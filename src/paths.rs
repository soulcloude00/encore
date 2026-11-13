//! XDG directory resolution. Empty and relative overrides are ignored.
use std::path::{Path, PathBuf};

fn absolute(value: Option<std::ffi::OsString>) -> Option<PathBuf> {
    value.map(PathBuf::from).filter(|p| p.is_absolute())
}

fn base(key: &str, fallback: &str) -> PathBuf {
    resolve(std::env::var_os(key), std::env::var_os("HOME"), fallback)
}

fn resolve(
    value: Option<std::ffi::OsString>,
    home: Option<std::ffi::OsString>,
    fallback: &str,
) -> PathBuf {
    absolute(value)
        .or_else(|| absolute(home).map(|h| h.join(fallback)))
        .unwrap_or_else(|| fallback_root().join(fallback))
}

fn fallback_root() -> PathBuf {
    // No persistent home: isolate this process in a private temporary directory.
    static FALLBACK: std::sync::OnceLock<tempfile::TempDir> = std::sync::OnceLock::new();
    FALLBACK
        .get_or_init(|| {
            tempfile::Builder::new().prefix("encore-home-").tempdir()
        .expect("Encore needs HOME, an absolute XDG directory, or a writable temporary directory")
        })
        .path()
        .to_path_buf()
}

pub fn config() -> PathBuf {
    base("XDG_CONFIG_HOME", ".config").join("encore")
}
pub fn cache() -> PathBuf {
    base("XDG_CACHE_HOME", ".cache").join("encore")
}
pub fn data() -> PathBuf {
    base("XDG_DATA_HOME", ".local/share").join("encore")
}
pub fn state() -> PathBuf {
    base("XDG_STATE_HOME", ".local/state")
}
pub fn models() -> PathBuf {
    absolute(std::env::var_os("ENCORE_MODEL_DIR")).unwrap_or_else(|| data().join("models"))
}

/// Only a single safe path component can identify cached media.
pub fn valid_media_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

pub fn report(label: &str, path: &Path) {
    println!("{label}: {}", path.display());
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn xdg_requires_absolute_paths() {
        assert_eq!(
            resolve(Some("relative".into()), Some("/home/test".into()), ".cache"),
            PathBuf::from("/home/test/.cache")
        );
        assert_eq!(
            resolve(Some("".into()), Some("/home/test".into()), ".cache"),
            PathBuf::from("/home/test/.cache")
        );
        assert_eq!(
            resolve(Some("/custom".into()), Some("/home/test".into()), ".cache"),
            PathBuf::from("/custom")
        );
    }
    #[test]
    fn cache_ids_cannot_escape_the_cache() {
        for id in ["", "../song", "/tmp/song", "a/b", "a\\b", ".", ".."] {
            assert!(!valid_media_id(id));
        }
        assert!(valid_media_id("abc_123-XYZ"));
    }
}
