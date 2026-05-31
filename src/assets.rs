//! Embedded asset management system.
//!
//! This module provides a clean API to load embedded assets from the `assets/` directory.
//! Assets are included in the binary at compile time using `rust-embed`.

use rust_embed::Embed;

/// Embedded assets from the `assets/` directory.
#[derive(Embed)]
#[folder = "assets/"]
pub struct Assets;

#[allow(dead_code)]
impl Assets {
    /// Load a text asset by path.
    ///
    /// # Arguments
    /// * `path` - Relative path within the assets folder (e.g., "shaders/model.wgsl")
    ///
    /// # Returns
    /// The file contents as a String, or None if not found.
    pub fn load_string(path: &str) -> Option<String> {
        Self::get(path).and_then(|file| String::from_utf8(file.data.into_owned()).ok())
    }

    /// Load a binary asset by path.
    ///
    /// # Arguments
    /// * `path` - Relative path within the assets folder (e.g., "models/cube.glb")
    ///
    /// # Returns
    /// The file contents as bytes, or None if not found.
    pub fn load_bytes(path: &str) -> Option<Vec<u8>> {
        Self::get(path).map(|file| file.data.into_owned())
    }

    /// Check if an asset exists at the given path.
    pub fn exists(path: &str) -> bool {
        Self::get(path).is_some()
    }

    /// List all asset paths.
    pub fn list_all() -> impl Iterator<Item = std::borrow::Cow<'static, str>> {
        Self::iter()
    }

    /// List asset paths matching a prefix.
    pub fn list_prefix(prefix: &str) -> Vec<String> {
        Self::iter()
            .filter(|path| path.starts_with(prefix))
            .map(|path| path.into_owned())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_assets_iter() {
        // Just ensure we can iterate - may be empty if no assets yet
        let _: Vec<_> = Assets::list_all().collect();
    }
}
