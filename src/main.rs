use clap::Parser;
use color_eyre::eyre::Result;
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use maestro::app::App;
use maestro::config::loader::load_config;
use maestro::event::bus::EventBus;
use maestro::event::types::AppEvent;
use ratatui::prelude::*;
use std::io::stdout;
use tracing::info;
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(name = "maestro", version, about = "TUI agent dashboard for Claude Code")]
struct Cli {
    /// Path to config file
    #[arg(short, long)]
    config: Option<std::path::PathBuf>,

    /// Log level (trace, debug, info, warn, error)
    #[arg(long, default_value = "info")]
    log_level: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;
    let cli = Cli::parse();

    // Load configuration
    let config = load_config(cli.config.as_deref())?;

    // Setup logging to file (not stdout — that's the TUI)
    let log_dir = maestro::config::loader::expand_tilde(&config.global.log_dir);
    std::fs::create_dir_all(&log_dir).ok(); // Best effort
    let file_appender = tracing_appender::rolling::daily(&log_dir, "maestro.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new(&cli.log_level)),
        )
        .with_writer(non_blocking)
        .with_ansi(false)
        .init();

    info!("Maestro starting");
    info!(
        "Config: {} projects, {} templates",
        config.project.len(),
        config.template.len(),
    );

    // Panic hook to restore terminal
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let _ = stdout().execute(DisableMouseCapture);
        let _ = disable_raw_mode();
        let _ = stdout().execute(LeaveAlternateScreen);
        original_hook(panic_info);
    }));

    // Terminal setup
    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;
    if config.ui.mouse_enabled {
        stdout().execute(EnableMouseCapture)?;
    }
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;
    terminal.clear()?;

    // Create an unbounded channel for PTY events → App
    // PTY controllers need unbounded senders (blocking read loops can't await).
    // We bridge unbounded → bounded via a forwarding task.
    let (pty_tx, mut pty_rx) = tokio::sync::mpsc::unbounded_channel::<AppEvent>();

    // Create event bus and start background tasks (input, timers)
    let mut event_bus = EventBus::new();
    event_bus.start(config.ui.fps, config.global.state_check_interval_ms);

    // Bridge: forward unbounded PTY events into the bounded event bus
    let bus_tx = event_bus.get_sender();
    tokio::spawn(async move {
        while let Some(event) = pty_rx.recv().await {
            if bus_tx.send(event).await.is_err() {
                break;
            }
        }
    });

    // Create and run the app
    let mut app = App::new(config, pty_tx);
    let result = app.run(&mut terminal, &mut event_bus).await;

    // Terminal teardown (always runs, even on error)
    let _ = stdout().execute(DisableMouseCapture);
    disable_raw_mode()?;
    stdout().execute(LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    info!("Maestro exited");
    result
}
