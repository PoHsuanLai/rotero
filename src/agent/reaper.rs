//! Tracks spawned ACP agent child processes so they can be killed even when the
//! app exits via a signal (Cmd+Q, Ctrl+C, `dx serve` hot-reload → SIGTERM), where
//! Rust destructors (and thus `RawAcpConnection`'s `Drop`) never run.
//!
//! Each child is spawned into its own process group (see `connection.rs`
//! `pre_exec(setsid)`), so we kill the whole group by its negative PID — this
//! also reaps any grandchildren the node agent spawned. SIGKILL of the parent
//! still can't be caught by anything, but that is the only uncatchable case.
//!
//! Signal reaping is Unix-only. Windows has no equivalent of process groups
//! addressed by negative PID, nor of installing a handler that re-raises with
//! default semantics, so the signal machinery is `#[cfg(unix)]` and its caller
//! in `main` is gated to match. On Windows, cleanup falls back to
//! `RawAcpConnection`'s `Drop`, which still kills the child directly.

use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};

fn registry() -> &'static Mutex<HashSet<i32>> {
    static REG: OnceLock<Mutex<HashSet<i32>>> = OnceLock::new();
    REG.get_or_init(|| Mutex::new(HashSet::new()))
}

/// Record a live child PID (which is also its process-group id).
pub(crate) fn register(pid: i32) {
    if pid > 0 {
        registry().lock().unwrap().insert(pid);
    }
}

/// Forget a child PID once we've reaped it ourselves.
pub(crate) fn unregister(pid: i32) {
    registry().lock().unwrap().remove(&pid);
}

/// Kill every tracked process group. Async-signal-safety note: this calls only
/// `kill(2)` and touches a `Mutex` — acceptable here because our own signal
/// handler runs on a normal thread context (see `install_signal_handler`) rather
/// than a raw async-signal context.
#[cfg(unix)]
fn kill_all() {
    let pids: Vec<i32> = registry().lock().unwrap().iter().copied().collect();
    for pid in pids {
        // Negative pid → signal the whole process group.
        unsafe {
            libc::kill(-pid, libc::SIGKILL);
        }
    }
}

/// Installs handlers for SIGTERM/SIGINT/SIGHUP that reap tracked children and
/// then restore the default action and re-raise, so the process still exits with
/// normal semantics. Call once at startup.
#[cfg(unix)]
pub(crate) fn install_signal_handler() {
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
