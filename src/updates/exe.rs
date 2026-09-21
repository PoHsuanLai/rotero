use std::path::PathBuf;
use std::sync::OnceLock;

/// Path of this process's executable, captured before any update replaces it.
///
/// On Linux, `current_exe()` after `self-replace` becomes `$path (deleted)` and
/// restarting from that string fails. Snapshot once at startup.
static RUNNING_EXE: OnceLock<PathBuf> = OnceLock::new();

/// Remember the running executable path. Call once from `main` before launch.
pub fn remember_running_exe() {
    if let Ok(exe) = std::env::current_exe() {
        let _ = RUNNING_EXE.set(strip_deleted_suffix(exe));
    }
}

/// The executable to relaunch after an update. Prefers the startup snapshot.
pub fn running_exe() -> Option<PathBuf> {
    RUNNING_EXE
        .get()
        .cloned()
        .or_else(|| std::env::current_exe().ok().map(strip_deleted_suffix))
}

pub(super) fn strip_deleted_suffix(path: PathBuf) -> PathBuf {
    const SUFFIX: &str = " (deleted)";
    match path.to_str().and_then(|s| s.strip_suffix(SUFFIX)) {
        Some(stripped) => PathBuf::from(stripped),
        None => path,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn strip_deleted_suffix_removes_kernel_marker() {
        let path = PathBuf::from("/home/me/.local/lib/rotero/rotero (deleted)");
        assert_eq!(
            strip_deleted_suffix(path),
            PathBuf::from("/home/me/.local/lib/rotero/rotero")
        );
    }

    #[test]
    fn strip_deleted_suffix_leaves_live_paths_alone() {
        let path = PathBuf::from("/home/me/.local/lib/rotero/rotero");
        assert_eq!(strip_deleted_suffix(path.clone()), path);
    }
}
