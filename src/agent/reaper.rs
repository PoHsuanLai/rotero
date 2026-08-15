//! Tracks spawned ACP agent child processes so they can be killed even when the
//! app exits via a signal (Cmd+Q, Ctrl+C, `dx serve` hot-reload → SIGTERM), where
//! Rust destructors (and thus `RawAcpConnection`'s `Drop`) never run.
//!
//! Each child is spawned as a process-group leader (see `connection.rs`), so
//! killing the group by its negative PID also reaps any grandchildren the node
//! agent spawned. SIGKILL of the parent still can't be caught by anything, but
//! that is the only uncatchable case.
//!
//! Unix-only: this is about POSIX signal delivery. On Windows the equivalent
//! guarantee comes from the job object the child is spawned into, which the
//! kernel tears down with the process — no signal handler needed.
//!
//! # Async-signal-safety
//!
//! `handle_signal` runs in a real async-signal context, so everything it
//! touches must be async-signal-safe. That rules out locking a `Mutex` or
//! allocating: a signal arriving while [`register`] held a lock would deadlock
//! the handler, and the process would hang instead of exiting. The registry is
//! therefore a fixed-size array of `AtomicI32` — lock-free, allocation-free,
//! and safe to walk from a handler.

use std::sync::atomic::{AtomicI32, Ordering};

/// Maximum number of concurrently tracked agent processes. One ACP agent runs
/// at a time in practice; the slack covers switching between providers before
/// the old child is reaped.
const MAX_TRACKED: usize = 16;

/// Empty slot marker. Real PIDs are always > 0.
const EMPTY: i32 = 0;

/// Live child PIDs (each is also its process-group id).
///
/// A fixed array of atomics rather than a `Mutex<HashSet>` so the signal
/// handler can read it without locking or allocating.
static REGISTRY: [AtomicI32; MAX_TRACKED] = [const { AtomicI32::new(EMPTY) }; MAX_TRACKED];

/// Record a live child PID (which is also its process-group id).
///
/// Silently drops the PID if every slot is occupied — the child is still
/// reaped by `RawAcpConnection`'s `Drop` on a normal exit.
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

/// Forget a child PID once we've reaped it ourselves.
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

/// Kill every tracked process group.
///
/// Async-signal-safe: atomic swaps and `kill(2)` only — no locks, no
/// allocation. The swap also clears each slot, so a concurrent handler cannot
/// signal the same group twice.
#[cfg(unix)]
fn kill_all() {
    for slot in &REGISTRY {
        let pid = slot.swap(EMPTY, Ordering::AcqRel);
        if pid > 0 {
            // Negative pid → signal the whole process group.
            unsafe {
                libc::kill(-pid, libc::SIGKILL);
            }
        }
    }
}

/// Installs handlers for SIGTERM/SIGINT/SIGHUP that reap tracked children and
/// then restore the default action and re-raise, so the process still exits with
/// normal semantics. Call once at startup.
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
    // Restore default handler and re-raise so the process terminates as expected.
    unsafe {
        libc::signal(sig, libc::SIG_DFL);
        libc::raise(sig);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The registry is global and the test harness runs tests in parallel
    /// threads, so each test takes this lock for its duration.
    static TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Drain the registry so each test starts from a known state, and hold the
    /// lock until the guard is dropped at the end of the test.
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
        // One more than capacity: the overflow is dropped, not a panic.
        for pid in 1..=(MAX_TRACKED as i32 + 1) {
            register(pid);
        }
        assert_eq!(tracked().len(), MAX_TRACKED);
    }
}
