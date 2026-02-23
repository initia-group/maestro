//! PTY controller — manages reading/writing to a pseudo-terminal.
//!
//! Wraps the raw PTY file descriptors with async read loop and
//! synchronized write access.

use crate::agent::AgentId;
use crate::event::types::AppEvent;
use color_eyre::eyre::Result;
use portable_pty::{MasterPty, PtySize};
use std::io::Write;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::{debug, warn};

/// Manages async read/write for a single agent's PTY.
pub struct PtyController {
    /// Agent this controller belongs to.
    agent_id: AgentId,

    /// The master end of the PTY, wrapped for thread-safe write access.
    /// Using `Arc<Mutex<>>` because portable-pty's Write impl is `!Send`
    /// and we need to write from the main tokio thread.
    writer: Arc<Mutex<Box<dyn Write + Send>>>,

    /// The master PTY handle (for resize operations).
    master: Arc<Mutex<Box<dyn MasterPty + Send>>>,

    /// Handle to the background read task (for cancellation on shutdown).
    read_task: Option<JoinHandle<()>>,
}

impl PtyController {
    /// Create a new controller and start the background read loop.
    ///
    /// # Arguments
    /// * `agent_id` — identifies this agent in events.
    /// * `master` — the master end of the PTY.
    /// * `event_tx` — sender to the central event bus.
    pub fn new(
        agent_id: AgentId,
        master: Box<dyn MasterPty + Send>,
        event_tx: mpsc::UnboundedSender<AppEvent>,
    ) -> Result<Self> {
        // Get a reader handle first (cloneable — must be called before take_writer)
        let reader = master
            .try_clone_reader()
            .map_err(|e| color_eyre::eyre::eyre!("Failed to clone PTY reader: {e}"))?;

        // Take the writer handle (may only be called once)
        let writer = master
            .take_writer()
            .map_err(|e| color_eyre::eyre::eyre!("Failed to take PTY writer: {e}"))?;

        let writer = Arc::new(Mutex::new(writer));
        let master = Arc::new(Mutex::new(master));

        // Spawn the background read task
        let read_task = Self::spawn_read_task(agent_id, reader, event_tx);

        Ok(Self {
            agent_id,
            writer,
            master,
            read_task: Some(read_task),
        })
    }

    /// Spawn a blocking task that reads from the PTY and sends output events.
    fn spawn_read_task(
        agent_id: AgentId,
        mut reader: Box<dyn std::io::Read + Send>,
        event_tx: mpsc::UnboundedSender<AppEvent>,
    ) -> JoinHandle<()> {
        tokio::task::spawn_blocking(move || {
            let mut buf = [0u8; 4096];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => {
                        // EOF — child process has exited
                        debug!("PTY EOF for agent {agent_id}");
                        let _ = event_tx.send(AppEvent::PtyEof { agent_id });
                        break;
                    }
                    Ok(n) => {
                        let data = buf[..n].to_vec();
                        if event_tx
                            .send(AppEvent::PtyOutput { agent_id, data })
                            .is_err()
                        {
                            // Event bus is gone — app is shutting down
                            debug!(
                                "Event bus closed, PTY reader exiting for agent {agent_id}"
                            );
                            break;
                        }
                    }
                    Err(e) => {
                        // Read error — PTY is probably closed
                        if e.kind() != std::io::ErrorKind::BrokenPipe {
                            warn!("PTY read error for agent {agent_id}: {e}");
                        }
                        let _ = event_tx.send(AppEvent::PtyEof { agent_id });
                        break;
                    }
                }
            }
        })
    }

    /// Write bytes to the agent's PTY (sends input to the child process).
    /// This is synchronous and fast — just writes to a pipe.
    pub fn write(&self, data: &[u8]) -> Result<()> {
        let mut writer = self.writer.lock().unwrap();
        writer
            .write_all(data)
            .map_err(|e| {
                color_eyre::eyre::eyre!("PTY write error for agent {}: {e}", self.agent_id)
            })?;
        writer.flush().map_err(|e| {
            color_eyre::eyre::eyre!("PTY flush error for agent {}: {e}", self.agent_id)
        })?;
        Ok(())
    }

    /// Resize the PTY. Called when the terminal window or layout changes.
    pub fn resize(&self, size: PtySize) -> Result<()> {
        let master = self.master.lock().unwrap();
        master.resize(size).map_err(|e| {
            color_eyre::eyre::eyre!("PTY resize error for agent {}: {e}", self.agent_id)
        })?;
        Ok(())
    }

    /// Get the agent ID this controller belongs to.
    pub fn agent_id(&self) -> AgentId {
        self.agent_id
    }

    /// Shut down the controller — abort the read task.
    pub fn shutdown(&mut self) {
        if let Some(task) = self.read_task.take() {
            task.abort();
        }
    }
}

impl Drop for PtyController {
    fn drop(&mut self) {
        self.shutdown();
    }
}
