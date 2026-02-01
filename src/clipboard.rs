//! Clipboard access for image and file paste support.
//!
//! Provides direct system clipboard access using arboard, supporting:
//! - Text content
//! - Image content (saved to temp files as PNG)
//! - File paths (platform-dependent)

use arboard::Clipboard;
use std::path::PathBuf;

/// Content types that can be read from the clipboard.
#[derive(Debug)]
pub enum ClipboardContent {
    /// Text content ready for paste.
    Text(String),
    /// Image saved to a temporary file.
    Image(PathBuf),
    /// No usable content in clipboard.
    Empty,
}

/// Handler for clipboard operations.
pub struct ClipboardHandler {
    clipboard: Clipboard,
    temp_dir: PathBuf,
}

impl ClipboardHandler {
    /// Create a new clipboard handler.
    ///
    /// Creates temp directory for image files if it doesn't exist.
    pub fn new() -> Result<Self, arboard::Error> {
        let clipboard = Clipboard::new()?;
        let temp_dir = std::env::temp_dir().join("claudewrapper");
        std::fs::create_dir_all(&temp_dir).ok();
        Ok(Self {
            clipboard,
            temp_dir,
        })
    }

    /// Get clipboard content, preferring image over text.
    ///
    /// If clipboard contains an image, saves it to a temp file and returns the path.
    /// Otherwise returns text content if available.
    pub fn get_content(&mut self) -> ClipboardContent {
        // Try image first
        if let Ok(image_data) = self.clipboard.get_image() {
            if let Some(path) = self.save_image(&image_data) {
                return ClipboardContent::Image(path);
            }
        }

        // Fall back to text
        if let Ok(text) = self.clipboard.get_text() {
            if !text.trim().is_empty() {
                return ClipboardContent::Text(text);
            }
        }

        ClipboardContent::Empty
    }

    /// Get text content only (for normal paste operations).
    #[allow(dead_code)]
    pub fn get_text(&mut self) -> Option<String> {
        self.clipboard
            .get_text()
            .ok()
            .filter(|t| !t.trim().is_empty())
    }

    /// Check if clipboard has image content without consuming it.
    pub fn has_image(&mut self) -> bool {
        self.clipboard.get_image().is_ok()
    }

    /// Save image data to a temporary PNG file.
    fn save_image(&self, image_data: &arboard::ImageData) -> Option<PathBuf> {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()?
            .as_millis();
        let filename = format!("paste_{}.png", timestamp);
        let path = self.temp_dir.join(&filename);

        // Convert RGBA bytes to image and save as PNG
        let img = image::RgbaImage::from_raw(
            image_data.width as u32,
            image_data.height as u32,
            image_data.bytes.to_vec(),
        )?;

        img.save(&path).ok()?;
        Some(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clipboard_handler_creation() {
        // This may fail in CI without display, so just check it compiles
        let _handler = ClipboardHandler::new();
    }
}
