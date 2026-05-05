//! User-level config persistence.
//!
//! Stored as `config.json` in the Tauri app config dir. Holds:
//! - `hd_root`: the path the user picked for their library
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
    pub fn load(config_dir: &Path) -> AppResult<Self> {
        let path = config_dir.join(CONFIG_FILENAME);
        if !path.exists() {
            return Ok(Self {
                ui_language: default_language(),
                ..Default::default()
            });
        }
        let bytes = std::fs::read(&path).map_err(AppError::from)?;
        let cfg: UserConfig = serde_json::from_slice(&bytes)
            .map_err(|e| AppError::Other(format!("config decode: {e}")))?;
        Ok(cfg)
    }

    /// Save atomically (write tmp, rename).
    pub fn save(&self, config_dir: &Path) -> AppResult<()> {
        std::fs::create_dir_all(config_dir).map_err(AppError::from)?;
        let path = config_dir.join(CONFIG_FILENAME);
        let tmp = config_dir.join(format!("{CONFIG_FILENAME}.tmp"));
        let bytes = serde_json::to_vec_pretty(self)
            .map_err(|e| AppError::Other(format!("config encode: {e}")))?;
        std::fs::write(&tmp, bytes).map_err(AppError::from)?;
        std::fs::rename(&tmp, &path).map_err(AppError::from)?;
        Ok(())
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
        assert!(cfg.hd_root.is_none());
        assert!(cfg.tmdb_api_key.is_none());
    }

    #[test]
    fn save_then_load_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let mut cfg = UserConfig::default();
        cfg.hd_root = Some(PathBuf::from("D:/Movies"));
        cfg.tmdb_api_key = Some("abc".to_string());
        cfg.ui_language = "pt-BR".to_string();
        cfg.compression_codec = Some("hevc_amf".to_string());
        cfg.save(tmp.path()).unwrap();

        let loaded = UserConfig::load(tmp.path()).unwrap();
        assert_eq!(loaded.hd_root, Some(PathBuf::from("D:/Movies")));
        assert_eq!(loaded.tmdb_api_key.as_deref(), Some("abc"));
        assert_eq!(loaded.ui_language, "pt-BR");
        assert_eq!(loaded.compression_codec.as_deref(), Some("hevc_amf"));
    }
}
