//! Diagnostic tool: prints raw crossterm key events.
//!
//! Run with: cargo run --example keytest
//! Press Shift+Tab to see how it's reported. Ctrl+C to quit.

use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use std::io::{self, Write};

fn main() -> io::Result<()> {
    enable_raw_mode()?;
    print!("Maestro key event diagnostic\r\n");
    print!("Press keys to see crossterm events. Ctrl+C to quit.\r\n");
    print!("Try pressing Shift+Tab — expect BackTab.\r\n\r\n");

    loop {
        match event::read()? {
            Event::Key(key) => {
                print!(
                    "KeyEvent {{ code: {:?}, modifiers: {:?}, kind: {:?} }}\r\n",
                    key.code, key.modifiers, key.kind
                );
                io::stdout().flush()?;

                if key.code == KeyCode::Char('c')
                    && key.modifiers.contains(KeyModifiers::CONTROL)
                {
                    break;
                }
            }
            _ => {}
        }
    }

    disable_raw_mode()?;
    print!("\r\nDone.\r\n");
    Ok(())
}
