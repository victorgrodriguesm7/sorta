//! User-level config persistence.
//!
//! Stored as `config.json` in the Tauri app config dir. Holds:
//! - `hd_roots`: every library drive the user has registered
//! - `hd_root`: legacy single-drive field, kept populated as the
//!   "primary" drive (first entry of `hd_roots`) for back-compat
//!   with code paths that still assume one active library
//! - `tmdb_api_key`: the user's TMDB v3 API key
//! - `ui_language`: the chosen UI locale
//!
//! NOT to be confused with the per-HD `settings` table (genre translations,
//! folder labels), which lives in `<HD root>/sorta.db`.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::error::{AppError, AppResult};

const CONFIG_FILENAME: &str = "config.json";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UserConfig {
    /// Every HD root the user has registered. The library UI merges
    /// content across the whole list. Cataloging / linking /
    /// re-cataloging always target the originating drive (the row
    /// carries its own `drive_root`), so writes never cross HDs.
    #[serde(default)]
    pub hd_roots: Vec<PathBuf>,

    /// Primary / "active" drive. Kept in sync with `hd_roots[0]`
    /// after [normalize] runs so legacy single-drive code paths keep
    /// working while the multi-drive refactor lands incrementally.
    /// Pre–multi-drive configs that only have this field auto-promote
    /// to `hd_roots` on load.
    pub hd_root: Option<PathBuf>,

    pub tmdb_api_key: Option<String>,
    #[serde(default = "default_language")]
    pub ui_language: String,
    /// Last-chosen compression encoder (serde tag of `Codec`, e.g.
    /// "hevc", "h264", "hevc_nvenc", "hevc_qsv", "hevc_amf"). When
    /// present, the compression dialog uses it instead of auto-picking
    /// a hardware encoder.
    #[serde(default)]
    pub compression_codec: Option<String>,
}

fn default_language() -> String {
    "en-US".to_string()
}

impl UserConfig {
    /// Load from `<config_dir>/config.json`. Missing file → default.
    ///
    /// Legacy single-drive configs (`hd_root: "/path"` with no
    /// `hd_roots` array) are promoted to a one-element `hd_roots`
    /// during load.
    pub fn load(config_dir: &Path) -> AppResult<Self> {
        let path = config_dir.join(CONFIG_FILENAME);
        if !path.exists() {
            return Ok(Self {
                ui_language: default_language(),
                ..Default::default()
            });
        }
        let bytes = std::fs::read(&path).map_err(AppError::from)?;
        let mut cfg: UserConfig = serde_json::from_slice(&bytes)
            .map_err(|e| AppError::Other(format!("config decode: {e}")))?;
        cfg.normalize();
        Ok(cfg)
    }

    /// Save atomically (write tmp, rename).
    pub fn save(&self, config_dir: &Path) -> AppResult<()> {
        std::fs::create_dir_all(config_dir).map_err(AppError::from)?;
        let path = config_dir.join(CONFIG_FILENAME);
        let tmp = config_dir.join(format!("{CONFIG_FILENAME}.tmp"));
        // Round-trip through normalize() so the on-disk shape is
        // always consistent — `hd_root` stays in sync with
        // `hd_roots[0]`, and an empty list clears the legacy field.
        let mut out = self.clone();
        out.normalize();
        let bytes = serde_json::to_vec_pretty(&out)
            .map_err(|e| AppError::Other(format!("config encode: {e}")))?;
        std::fs::write(&tmp, bytes).map_err(AppError::from)?;
        std::fs::rename(&tmp, &path).map_err(AppError::from)?;
        Ok(())
    }

    /// Append [path] to `hd_roots` if not already present.
    /// Returns true if the list actually changed.
    pub fn add_hd_root(&mut self, path: PathBuf) -> bool {
        if self.hd_roots.iter().any(|p| p == &path) {
            return false;
        }
        self.hd_roots.push(path);
        self.normalize();
        true
    }

    /// Remove [path] from `hd_roots`. Returns true if the list changed.
    pub fn remove_hd_root(&mut self, path: &Path) -> bool {
        let before = self.hd_roots.len();
        self.hd_roots.retain(|p| p != path);
        let changed = self.hd_roots.len() != before;
        self.normalize();
        changed
    }

    /// Reconcile `hd_root` (primary) with `hd_roots` (full list).
    ///   - empty `hd_roots` + Some `hd_root` → promote legacy.
    ///   - non-empty `hd_roots` → primary is always the first entry.
    ///   - empty list and empty primary → both stay None / [].
    fn normalize(&mut self) {
        if self.hd_roots.is_empty() {
            if let Some(legacy) = self.hd_root.take() {
                self.hd_roots.push(legacy.clone());
                self.hd_root = Some(legacy);
            }
        } else {
            self.hd_root = Some(self.hd_roots[0].clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn missing_config_returns_defaults() {
        let tmp = TempDir::new().unwrap();
        let cfg = UserConfig::load(tmp.path()).unwrap();
        assert_eq!(cfg.ui_language, "en-US");
        assert!(cfg.hd_roots.is_empty());
        assert!(cfg.hd_root.is_none());
        assert!(cfg.tmdb_api_key.is_none());
    }

    #[test]
    fn save_then_load_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let mut cfg = UserConfig::default();
        cfg.hd_roots = vec![PathBuf::from("D:/Movies"), PathBuf::from("C:/Backup")];
        cfg.tmdb_api_key = Some("abc".to_string());
        cfg.ui_language = "pt-BR".to_string();
        cfg.compression_codec = Some("hevc_amf".to_string());
        cfg.save(tmp.path()).unwrap();

        let loaded = UserConfig::load(tmp.path()).unwrap();
        assert_eq!(loaded.hd_roots, vec![PathBuf::from("D:/Movies"), PathBuf::from("C:/Backup")]);
        // Primary stays in sync with the first entry for legacy callers.
        assert_eq!(loaded.hd_root, Some(PathBuf::from("D:/Movies")));
        assert_eq!(loaded.tmdb_api_key.as_deref(), Some("abc"));
        assert_eq!(loaded.ui_language, "pt-BR");
        assert_eq!(loaded.compression_codec.as_deref(), Some("hevc_amf"));
    }

    #[test]
    fn legacy_hd_root_promotes_to_hd_roots_on_load() {
        let tmp = TempDir::new().unwrap();
        let legacy = r#"{
            "hd_root": "D:/Movies",
            "tmdb_api_key": "abc",
            "ui_language": "en-US"
        }"#;
        std::fs::write(tmp.path().join(CONFIG_FILENAME), legacy).unwrap();

        let loaded = UserConfig::load(tmp.path()).unwrap();
        assert_eq!(loaded.hd_roots, vec![PathBuf::from("D:/Movies")]);
        // Primary is preserved for legacy callers.
        assert_eq!(loaded.hd_root, Some(PathBuf::from("D:/Movies")));
    }

    #[test]
    fn add_and_remove_hd_root_are_idempotent() {
        let mut cfg = UserConfig::default();
        assert!(cfg.add_hd_root(PathBuf::from("D:/Movies")));
        assert!(!cfg.add_hd_root(PathBuf::from("D:/Movies")), "duplicate add must no-op");
        assert!(cfg.add_hd_root(PathBuf::from("E:/Backup")));
        assert_eq!(cfg.hd_roots.len(), 2);
        // Primary tracks the first entry.
        assert_eq!(cfg.hd_root, Some(PathBuf::from("D:/Movies")));

        assert!(cfg.remove_hd_root(Path::new("D:/Movies")));
        assert!(!cfg.remove_hd_root(Path::new("D:/Movies")), "second remove must no-op");
        assert_eq!(cfg.hd_roots, vec![PathBuf::from("E:/Backup")]);
        // Primary slides over to the next entry.
        assert_eq!(cfg.hd_root, Some(PathBuf::from("E:/Backup")));
    }

    #[test]
    fn empty_list_clears_primary() {
        let mut cfg = UserConfig::default();
        cfg.add_hd_root(PathBuf::from("D:/Movies"));
        assert!(cfg.remove_hd_root(Path::new("D:/Movies")));
        assert!(cfg.hd_roots.is_empty());
        assert!(cfg.hd_root.is_none());
    }
}
