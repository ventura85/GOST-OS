//! Atomic file replacement.
//!
//! Configuration is written to a temporary file in the *same directory*, flushed
//! to disk, and then renamed over the target. `rename(2)` within one filesystem is
//! atomic, so a reader either sees the whole old file or the whole new one — never
//! a truncated one.
//!
//! This matters more here than in most programs: a crash or a power cut while
//! saving the tab layout must not leave the user with an unparseable config and a
//! shell that will not start. Old hardware (D-027) makes an unexpected power loss
//! a realistic event, not a theoretical one.

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::Path;

/// Write `contents` to `path`, replacing any existing file atomically.
///
/// Creates the parent directory if it does not exist.
pub fn write(path: &Path, contents: &str) -> io::Result<()> {
    let dir = path.parent().ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "path has no parent directory")
    })?;
    fs::create_dir_all(dir)?;

    // The temporary file must live in the same directory as the target: rename is
    // only atomic within a filesystem, and /tmp is often a different one.
    let tmp = temp_path(path);

    {
        let mut f = File::create(&tmp)?;
        f.write_all(contents.as_bytes())?;
        // Without this the rename can land before the data does, and a power cut
        // leaves an empty file where the config used to be.
        f.sync_all()?;
    }

    match fs::rename(&tmp, path) {
        Ok(()) => {}
        Err(e) => {
            // Leaving stray temp files behind would slowly litter the config dir.
            let _ = fs::remove_file(&tmp);
            return Err(e);
        }
    }

    // Also flush the directory entry, so the rename itself survives a power cut.
    // Failure here is not fatal: the data is already on disk.
    if let Ok(d) = File::open(dir) {
        let _ = d.sync_all();
    }
    Ok(())
}

fn temp_path(path: &Path) -> std::path::PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(format!(".tmp.{}", std::process::id()));
    path.with_file_name(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_and_replaces() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");

        write(&path, "first").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "first");

        write(&path, "second").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "second");
    }

    #[test]
    fn creates_missing_parent_directories() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a/b/c/config.toml");
        write(&path, "x").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "x");
    }

    #[test]
    fn leaves_no_temporary_file_behind() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        write(&path, "x").unwrap();

        let leftovers: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(Result::ok)
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.contains(".tmp."))
            .collect();
        assert!(leftovers.is_empty(), "stray temp files: {leftovers:?}");
    }

    #[test]
    fn the_temporary_file_is_a_sibling_of_the_target() {
        // Not /tmp: rename across filesystems is not atomic and would fail here.
        let target = Path::new("/home/user/.config/gostui/config.toml");
        assert_eq!(temp_path(target).parent(), target.parent());
    }
}
