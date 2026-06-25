//! Dev-only diagnostic: is the locally served `ui/dist` bundle missing or older than `ui/src`?
//!
//! `cargo run` serves the prebuilt SPA from `ui/dist` but does not rebuild it. This lets `main.rs` warn
//! at startup when that bundle is missing or stale relative to `ui/src`, so a developer who edited the
//! frontend without running `npm --prefix ui run build` (or the Vite dev server) gets a clear signal
//! instead of a silently stale UI. `ui/dist` is a gitignored build artifact (never committed), so
//! "newest ui/src mtime > newest ui/dist mtime" reliably means "source edited since the last build".

use std::io;
use std::path::Path;
use std::time::SystemTime;

/// State of the local `ui/dist` bundle relative to `ui/src`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiDistStatus {
    /// `ui/dist` is at least as new as `ui/src`.
    Fresh,
    /// `ui/src` has a file newer than anything in `ui/dist`.
    Stale,
    /// `ui/dist` is absent or empty.
    MissingDist,
    /// Cannot tell: no readable `ui/src`, or an fs error reading either tree.
    Unknown,
}

/// Best-effort comparison of the newest file mtime under `ui_src` vs `ui_dist`.
/// Never panics, never logs. See [`UiDistStatus`] for result meanings.
pub fn ui_dist_status(ui_src: &Path, ui_dist: &Path) -> UiDistStatus {
    let src = match newest_mtime(ui_src) {
        Ok(Some(t)) => t,
        // No readable source baseline (absent/empty src, or an fs error): cannot judge staleness.
        Ok(None) | Err(_) => return UiDistStatus::Unknown,
    };
    match newest_mtime(ui_dist) {
        Ok(Some(dist)) => {
            if src > dist {
                UiDistStatus::Stale
            } else {
                UiDistStatus::Fresh
            }
        }
        Ok(None) => UiDistStatus::MissingDist,
        Err(_) => UiDistStatus::Unknown,
    }
}

/// Newest file modification time anywhere under `dir`, recursing into subdirectories.
/// `Ok(None)` means the directory is absent or contains no files; `Err` is any other fs error
/// (an unreadable directory, a non-directory at `dir`, or a metadata error).
fn newest_mtime(dir: &Path) -> io::Result<Option<SystemTime>> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    // `Option<SystemTime>` is ordered with `None` < `Some`, so `.max` cleanly merges candidates.
    let mut newest: Option<SystemTime> = None;
    for entry in entries {
        let entry = entry?;
        let candidate = if entry.file_type()?.is_dir() {
            newest_mtime(&entry.path())?
        } else {
            Some(entry.metadata()?.modified()?)
        };
        newest = newest.max(candidate);
    }
    Ok(newest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File};
    use std::path::PathBuf;
    use std::time::{Duration, SystemTime};

    /// Self-cleaning unique scratch dir under the system temp dir (no `tempfile` dependency).
    struct TempDir(PathBuf);
    impl TempDir {
        fn new(tag: &str) -> Self {
            use std::sync::atomic::{AtomicU32, Ordering};
            static N: AtomicU32 = AtomicU32::new(0);
            let n = N.fetch_add(1, Ordering::Relaxed);
            let p = std::env::temp_dir().join(format!(
                "labeler-uifresh-{}-{}-{}",
                std::process::id(),
                tag,
                n
            ));
            fs::create_dir_all(&p).unwrap();
            TempDir(p)
        }
        fn path(&self) -> &Path {
            &self.0
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    /// Create `dir/name` (making `dir`) with a fixed modification time.
    fn write_file(dir: &Path, name: &str, mtime: SystemTime) {
        fs::create_dir_all(dir).unwrap();
        let f = File::create(dir.join(name)).unwrap();
        f.set_modified(mtime).unwrap();
    }

    // Two well-separated fixed instants so comparisons are deterministic (no sleeps).
    fn t_old() -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000)
    }
    fn t_new() -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(2_000_000)
    }

    #[test]
    fn fresh_when_dist_newer_than_src() {
        let tmp = TempDir::new("fresh");
        let src = tmp.path().join("ui/src");
        let dist = tmp.path().join("ui/dist");
        write_file(&src, "App.tsx", t_old());
        write_file(&dist, "index.html", t_new());
        assert_eq!(ui_dist_status(&src, &dist), UiDistStatus::Fresh);
    }

    #[test]
    fn stale_when_src_newer_than_dist() {
        let tmp = TempDir::new("stale");
        let src = tmp.path().join("ui/src");
        let dist = tmp.path().join("ui/dist");
        write_file(&dist, "index.html", t_old());
        write_file(&src, "App.tsx", t_new());
        assert_eq!(ui_dist_status(&src, &dist), UiDistStatus::Stale);
    }

    #[test]
    fn missing_dist_when_dist_absent() {
        let tmp = TempDir::new("missing");
        let src = tmp.path().join("ui/src");
        let dist = tmp.path().join("ui/dist"); // never created
        write_file(&src, "App.tsx", t_old());
        assert_eq!(ui_dist_status(&src, &dist), UiDistStatus::MissingDist);
    }

    #[test]
    fn missing_dist_when_dist_empty() {
        let tmp = TempDir::new("empty");
        let src = tmp.path().join("ui/src");
        let dist = tmp.path().join("ui/dist");
        write_file(&src, "App.tsx", t_old());
        fs::create_dir_all(&dist).unwrap(); // exists but contains no files
        assert_eq!(ui_dist_status(&src, &dist), UiDistStatus::MissingDist);
    }

    #[test]
    fn unknown_when_src_absent() {
        let tmp = TempDir::new("nosrc");
        let src = tmp.path().join("ui/src"); // never created
        let dist = tmp.path().join("ui/dist");
        write_file(&dist, "index.html", t_new());
        assert_eq!(ui_dist_status(&src, &dist), UiDistStatus::Unknown);
    }

    #[test]
    fn unknown_when_dist_is_a_file_not_dir() {
        let tmp = TempDir::new("distfile");
        let src = tmp.path().join("ui/src");
        write_file(&src, "App.tsx", t_old());
        let dist = tmp.path().join("dist-as-file");
        File::create(&dist).unwrap(); // a regular file, not a directory
        assert_eq!(ui_dist_status(&src, &dist), UiDistStatus::Unknown);
    }
}
