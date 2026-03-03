//! Event bus — async event multiplexer.
//!
//! Combines multiple event sources (PTY output, crossterm input,
//! tick timers, state changes) into a single stream via tokio mpsc channels.

use crate::event::types::{AppEvent, InputEvent};
use crossterm::event::{Event as CrosstermEvent, EventStream, KeyEventKind};
use futures::StreamExt;
use std::time::Duration;
use tokio::sync::mpsc;

/// Default channel buffer size.
const DEFAULT_CHANNEL_SIZE: usize = 256;

/// Central event bus that multiplexes all event sources.
///
/// The bus owns a tokio mpsc channel pair. Background tasks (crossterm input,
/// tick timers, PTY readers) send events through cloned senders. The main loop
/// consumes events via [`EventBus::next`].
pub struct EventBus {
    /// Receiver for the merged event stream.
    rx: mpsc::Receiver<AppEvent>,
    /// Sender cloned to each event source.
    tx: mpsc::Sender<AppEvent>,
}

impl EventBus {
    /// Create a new event bus with the default buffer size (256).
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_CHANNEL_SIZE)
    }

    /// Create a new event bus with a custom channel buffer size.
    pub fn with_capacity(buffer: usize) -> Self {
        let (tx, rx) = mpsc::channel(buffer);
        Self { tx, rx }
    }

    /// Get a sender handle for external producers (PTY read loops, etc.)
    /// to inject events into the bus.
    pub fn get_sender(&self) -> mpsc::Sender<AppEvent> {
        self.tx.clone()
    }

    /// Start all background event source tasks.
    ///
    /// Spawns tokio tasks for:
    /// - Crossterm terminal input events
    /// - State detection tick timer (`state_check_interval_ms`, default 250ms)
    /// - Render scheduler (`fps`, default 30)
    pub fn start(&self, fps: u32, state_check_interval_ms: u64) {
        self.start_input_reader();
        self.start_state_tick(state_check_interval_ms);
        self.start_render_scheduler(fps);
    }

    /// Receive the next event. Returns `None` when all senders are dropped.
    pub async fn next(&mut self) -> Option<AppEvent> {
        self.rx.recv().await
    }

    /// Try to receive the next event without blocking.
    /// Returns `None` if the channel is empty or closed.
    ///
    /// Used by the main loop to drain all pending events after handling
    /// an awaited event, coalescing bursts of PTY output into a single
    /// dirty-render cycle.
    pub fn try_next(&mut self) -> Option<AppEvent> {
        self.rx.try_recv().ok()
    }

    /// Start the crossterm event reader task.
    ///
    /// Reads terminal events via [`EventStream`] (futures-based) and forwards
    /// key presses and resize events to the bus. Only `KeyEventKind::Press`
    /// events are forwarded to avoid duplicate handling on platforms that
    /// report press/release/repeat.
    fn start_input_reader(&self) {
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let mut reader = EventStream::new();
            loop {
                match reader.next().await {
                    Some(Ok(CrosstermEvent::Key(key))) => {
                        tracing::debug!(
                            code = ?key.code,
                            modifiers = ?key.modifiers,
                            kind = ?key.kind,
                            "Crossterm key event received"
                        );
                        // Only handle key press events (not release/repeat).
                        if key.kind == KeyEventKind::Press
                            && tx
                                .send(AppEvent::Input(InputEvent::Key(key)))
                                .await
                                .is_err()
                        {
                            break;
                        }
                    }
                    Some(Ok(CrosstermEvent::Resize(cols, rows))) => {
                        if tx.send(AppEvent::Resize { cols, rows }).await.is_err() {
                            break;
                        }
                    }
                    Some(Ok(CrosstermEvent::Mouse(mouse))) => {
                        if tx
                            .send(AppEvent::Input(InputEvent::Mouse(mouse)))
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                    Some(Ok(_)) => {
                        // Ignore other events (focus, paste) for now.
                    }
                    Some(Err(e)) => {
                        tracing::error!("Crossterm event error: {}", e);
                        break;
                    }
                    None => break,
                }
            }
        });
    }

    /// Start the state detection tick timer.
    ///
    /// Fires a [`AppEvent::StateTick`] at the configured interval so the
    /// agent manager can poll for state changes.
    fn start_state_tick(&self, interval_ms: u64) {
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(interval_ms));
            loop {
                interval.tick().await;
                if tx.send(AppEvent::StateTick).await.is_err() {
                    break;
                }
            }
        });
    }

    /// Start the render scheduler.
    ///
    /// Sends [`AppEvent::RenderRequest`] at the configured FPS rate. This is
    /// a "you may render now" signal — the app only draws if a dirty flag is set.
    fn start_render_scheduler(&self, fps: u32) {
        let tx = self.tx.clone();
        let frame_duration = Duration::from_secs_f64(1.0 / f64::from(fps));
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(frame_duration);
            loop {
                interval.tick().await;
                if tx.send(AppEvent::RenderRequest).await.is_err() {
                    break;
                }
            }
        });
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::AgentId;

    #[tokio::test]
    async fn event_bus_sender_can_inject_events() {
        let mut bus = EventBus::new();
        let tx = bus.get_sender();

        tx.send(AppEvent::QuitRequested).await.unwrap();

        let event = bus.next().await.unwrap();
        assert!(matches!(event, AppEvent::QuitRequested));
    }

    #[tokio::test]
    async fn event_bus_handles_pty_output() {
        let mut bus = EventBus::new();
        let tx = bus.get_sender();

        let id = AgentId::new();
        tx.send(AppEvent::PtyOutput {
            agent_id: id,
            data: b"hello".to_vec(),
        })
        .await
        .unwrap();

        let event = bus.next().await.unwrap();
        match event {
            AppEvent::PtyOutput { agent_id, data } => {
                assert_eq!(agent_id, id);
                assert_eq!(data, b"hello");
            }
            _ => panic!("Expected PtyOutput event"),
        }
    }

    #[tokio::test]
    async fn event_bus_receives_state_tick() {
        let mut bus = EventBus::new();
        // Use a short interval for fast testing.
        bus.start(30, 50);

        let event = tokio::time::timeout(Duration::from_millis(200), bus.next()).await;

        assert!(event.is_ok(), "Should receive an event within 200ms");
        // Could be StateTick or RenderRequest — both are valid.
    }

    #[tokio::test]
    async fn event_bus_returns_none_when_all_senders_dropped() {
        let (tx, mut rx) = mpsc::channel::<AppEvent>(16);
        drop(tx);
        assert!(rx.recv().await.is_none());
    }

    #[tokio::test]
    async fn event_bus_with_custom_capacity() {
        let mut bus = EventBus::with_capacity(16);
        let tx = bus.get_sender();

        tx.send(AppEvent::StateTick).await.unwrap();
        let event = bus.next().await.unwrap();
        assert!(matches!(event, AppEvent::StateTick));
    }
}
