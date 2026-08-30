//! Tracks the ACP agent process group so Ctrl+C / SIGTERM can kill it.
//!
//! The SDK puts the child in its own process group so Drop can reap `npx`/`uvx`
//! grandchildren. That also means SIGINT to Rotero never reaches the agent.
//! Drop still handles session switch and a clean quit; this registry is for the
//! case where the process dies without running destructors.
//!
//! The handler only does atomic swaps and `kill(2)` — no locks, no allocation.

use std::sync::atomic::{AtomicI32, Ordering};

const MAX_TRACKED: usize = 16;
const EMPTY: i32 = 0;

static REGISTRY: [AtomicI32; MAX_TRACKED] = [const { AtomicI32::new(EMPTY) }; MAX_TRACKED];

pub(crate) fn register(pid: i32) {
    if pid <= 0 {
        return;
    }
    for slot in &REGISTRY {
        if slot
            .compare_exchange(EMPTY, pid, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            return;
        }
    }
    tracing::warn!("agent reaper registry full; {pid} will not be signal-reaped");
}

pub(crate) fn unregister(pid: i32) {
    if pid <= 0 {
        return;
    }
    for slot in &REGISTRY {
        if slot
            .compare_exchange(pid, EMPTY, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            return;
        }
    }
}

/// SIGKILL the process group whose leader is `pid`.
///
/// Safe to call if the group is already gone.
pub(crate) fn kill_group(pid: i32) {
    if pid <= 0 {
        return;
    }
    #[cfg(unix)]
    unsafe {
        libc::kill(-pid, libc::SIGKILL);
    }
}

#[cfg(unix)]
fn kill_all() {
    for slot in &REGISTRY {
        let pid = slot.swap(EMPTY, Ordering::AcqRel);
        kill_group(pid);
    }
}

/// Install SIGTERM/SIGINT/SIGHUP handlers that kill tracked agent groups, then
/// restore the default action and re-raise so the process still exits.
#[cfg(unix)]
pub(crate) fn install_signal_handler() {
    use std::sync::OnceLock;

    static INSTALLED: OnceLock<()> = OnceLock::new();
    if INSTALLED.set(()).is_err() {
        return;
    }
    let handler = handle_signal as extern "C" fn(i32) as *const () as libc::sighandler_t;
    unsafe {
        for sig in [libc::SIGTERM, libc::SIGINT, libc::SIGHUP] {
            libc::signal(sig, handler);
        }
    }
}

#[cfg(unix)]
extern "C" fn handle_signal(sig: i32) {
    kill_all();
    unsafe {
        libc::signal(sig, libc::SIG_DFL);
        libc::raise(sig);
    }
}

#[cfg(not(unix))]
pub(crate) fn install_signal_handler() {}

#[cfg(test)]
mod tests {
    use super::*;

    static TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn clear() -> std::sync::MutexGuard<'static, ()> {
        let guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        for slot in &REGISTRY {
            slot.store(EMPTY, Ordering::Release);
        }
        guard
    }

    fn tracked() -> Vec<i32> {
        REGISTRY
            .iter()
            .map(|s| s.load(Ordering::Acquire))
            .filter(|&p| p != EMPTY)
            .collect()
    }

    #[test]
    fn register_then_unregister_round_trips() {
        let _guard = clear();
        register(42);
        assert_eq!(tracked(), vec![42]);
        unregister(42);
        assert!(tracked().is_empty());
    }

    #[test]
    fn ignores_non_positive_pids() {
        let _guard = clear();
        register(0);
        register(-1);
        assert!(tracked().is_empty());
    }

    #[test]
    fn tracks_multiple_pids_and_removes_only_the_named_one() {
        let _guard = clear();
        for pid in [10, 20, 30] {
            register(pid);
        }
        unregister(20);
        let mut left = tracked();
        left.sort_unstable();
        assert_eq!(left, vec![10, 30]);
    }

    #[test]
    fn unregistering_an_unknown_pid_is_a_no_op() {
        let _guard = clear();
        register(7);
        unregister(999);
        assert_eq!(tracked(), vec![7]);
    }

    #[test]
    fn registry_saturates_without_panicking() {
        let _guard = clear();
        for pid in 1..=(MAX_TRACKED as i32 + 1) {
            register(pid);
        }
        assert_eq!(tracked().len(), MAX_TRACKED);
    }
}
