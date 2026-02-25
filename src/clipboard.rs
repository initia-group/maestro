//! Clipboard operations for text copy.

/// Copy text to the system clipboard.
///
/// Returns `Ok(())` on success, `Err` with a message if the clipboard
/// is unavailable (e.g. headless/SSH session without a display).
pub fn copy_to_clipboard(text: &str) -> Result<(), String> {
    let mut clipboard =
        arboard::Clipboard::new().map_err(|e| format!("Clipboard unavailable: {}", e))?;
    clipboard
        .set_text(text.to_owned())
        .map_err(|e| format!("Failed to copy: {}", e))
}
