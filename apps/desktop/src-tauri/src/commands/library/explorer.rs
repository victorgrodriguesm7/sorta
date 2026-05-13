//! `open_in_explorer` — reveal a path in the OS file manager.

use crate::error::{AppError, AppResult};

/// Reveal `path` in the OS file manager. Accepts a folder *or* a file;
/// when a file is given the manager opens its containing folder. We
/// shell out per-platform instead of pulling in `tauri-plugin-opener`
/// to keep the dependency set small — this command does one thing.
#[tauri::command]
pub async fn open_in_explorer(path: String) -> AppResult<()> {
    let p = std::path::PathBuf::from(&path);
    if !p.exists() {
        return Err(AppError::NotFound(format!("path does not exist: {path}")));
    }

    #[cfg(target_os = "windows")]
    {
        // Explorer.exe is picky: forward-slash paths (which the
        // frontend produces by joining `${hd_root}/${folder_path}`)
        // make it silently fall back to the Documents folder. We
        // normalize on the *string* form because `PathBuf` on Windows
        // preserves whatever separator was used to construct it —
        // the OS-level filesystem APIs accept either, but explorer
        // does not.
        let normalized = path.replace('/', "\\");
        std::process::Command::new("explorer")
            .arg(&normalized)
            .spawn()?;
        return Ok(());
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open").arg(&p).spawn()?;
        return Ok(());
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        // xdg-open opens a file in its associated app, which is the
        // opposite of what we want for a video file. So when `p` is a
        // file, target its parent.
        let target = if p.is_file() {
            p.parent().map(|x| x.to_path_buf()).unwrap_or(p.clone())
        } else {
            p.clone()
        };
        std::process::Command::new("xdg-open").arg(&target).spawn()?;
        Ok(())
    }
}
