//! Filesystem watcher built on `notify-debouncer-full`.
//!
//! The watcher emits coarse "something changed under root" events to a
//! tokio mpsc channel; downstream code is expected to debounce/coalesce
//! at the application level (e.g. trigger a re-scan after a quiet period).

use std::path::{Path, PathBuf};
use std::sync::mpsc as std_mpsc;
use std::time::Duration;

use notify::{RecursiveMode, Watcher};
use notify_debouncer_full::{new_debouncer, DebounceEventResult, Debouncer, FileIdMap};
use tokio::sync::mpsc;

use crate::error::{AppError, AppResult};

/// Coarse change notification — the affected paths.
#[derive(Debug, Clone)]
pub struct ChangeEvent {
    pub paths: Vec<PathBuf>,
}

/// A live watcher. Drop to stop watching.
pub struct WatcherHandle {
    _debouncer: Debouncer<notify::RecommendedWatcher, FileIdMap>,
}

/// Watch `root` recursively and forward debounced events into `tx`.
///
/// The internal debounce window is 1 second. Returns a handle that must
/// be kept alive for as long as you want to receive events.
pub fn watch(
    root: &Path,
    tx: mpsc::UnboundedSender<ChangeEvent>,
) -> AppResult<WatcherHandle> {
    if !root.exists() {
        return Err(AppError::InvalidPath(format!(
            "{} does not exist",
            root.display()
        )));
    }
    // notify-debouncer-full requires a std mpsc; we bridge it to tokio.
    let (raw_tx, raw_rx) = std_mpsc::channel::<DebounceEventResult>();
    let mut debouncer = new_debouncer(Duration::from_secs(1), None, raw_tx)
        .map_err(|e| AppError::Other(format!("watcher init: {e}")))?;
    debouncer
        .watcher()
        .watch(root, RecursiveMode::Recursive)
        .map_err(|e| AppError::Other(format!("watcher watch: {e}")))?;

    std::thread::spawn(move || {
        for result in raw_rx {
            let Ok(events) = result else { continue };
            let mut paths = Vec::new();
            for ev in events {
                paths.extend(ev.event.paths.clone());
            }
            if paths.is_empty() {
                continue;
            }
            if tx.send(ChangeEvent { paths }).is_err() {
                break; // receiver dropped
            }
        }
    });

    Ok(WatcherHandle { _debouncer: debouncer })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;
    use tokio::time::{timeout, Duration as TDur};

    #[tokio::test]
    async fn watcher_emits_events_on_file_create() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().to_path_buf();
        let (tx, mut rx) = mpsc::unbounded_channel();
        let _handle = watch(&root, tx).expect("watch");

        // Give notify a moment to register the watch on Windows.
        tokio::time::sleep(TDur::from_millis(300)).await;

        // Create a file under root.
        fs::write(root.join("hello.txt"), b"hi").unwrap();

        // Debounce window is 1s; allow up to 4s for slow CI.
        let event = timeout(TDur::from_secs(4), rx.recv()).await;
        assert!(event.is_ok(), "no event arrived in time");
        let event = event.unwrap().expect("channel closed");
        assert!(!event.paths.is_empty());
    }

    #[tokio::test]
    async fn watch_errors_on_missing_root() {
        let tmp = TempDir::new().unwrap();
        let missing = tmp.path().join("nope");
        let (tx, _rx) = mpsc::unbounded_channel();
        assert!(matches!(watch(&missing, tx), Err(AppError::InvalidPath(_))));
    }
}
