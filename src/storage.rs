//! Backward-compatible settings/history storage with atomic replacement.
use crate::ytm::Track;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

pub fn config_dir() -> Option<PathBuf> {
    Some(crate::paths::config())
}
pub fn settings_path() -> Option<PathBuf> {
    Some(crate::paths::config().join("settings"))
}

pub fn setting(key: &str) -> Option<String> {
    let text = std::fs::read_to_string(settings_path()?).ok()?;
    text.lines().find_map(|l| {
        l.strip_prefix(&format!("{key}="))
            .map(|v| v.trim().to_string())
    })
}

pub fn load_history() -> Vec<Track> {
    let Ok(text) = std::fs::read_to_string(crate::paths::config().join("history.json")) else {
        return Vec::new();
    };
    let Ok(serde_json::Value::Array(items)) = serde_json::from_str(&text) else {
        return Vec::new();
    };
    items.iter().filter_map(Track::from_json).collect()
}

pub fn save_history(history: &[Track]) {
    let json = serde_json::Value::Array(history.iter().map(Track::to_json).collect());
    if let Err(e) = write_atomic(
        &crate::paths::config().join("history.json"),
        json.to_string().as_bytes(),
    ) {
        eprintln!("encore: couldn't save history: {e}");
    }
}

/// A failed write leaves the previous file intact. Tempfiles are private and on the same
/// filesystem as the destination; rename is atomic on the supported Linux platform.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("missing parent directory"))?;
    std::fs::create_dir_all(parent)?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    file.write_all(bytes)?;
    file.as_file().sync_all()?;
    file.persist(path).map_err(|e| e.error)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn replaces_existing_data_and_creates_parent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested/settings");
        write_atomic(&path, b"old").unwrap();
        write_atomic(&path, b"new").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"new");
        assert_eq!(
            std::fs::read_dir(path.parent().unwrap()).unwrap().count(),
            1
        );
    }
}
