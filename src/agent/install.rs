use std::path::PathBuf;

/// Run a command, killing it if it outruns `timeout`.
///
/// `Command::output()` waits forever; there is no timeout variant in std.
///
/// The pipes are drained on their own threads *while* the child runs. Reading
/// them only after it exits deadlocks: a pipe buffer holds around 64 KB, and a
/// child that fills it blocks on write and never exits, so the wait loop below
/// runs to its deadline and reports a timeout for a command that was working
/// normally. `npm install` on a cold cache easily writes that much progress
/// output. (`Command::output()`, which this replaced, drains concurrently and
/// did not have that problem.)
pub(crate) fn run_with_timeout(
    cmd: &mut std::process::Command,
    timeout: std::time::Duration,
) -> Result<std::process::Output, String> {
    let mut child = cmd
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to run {:?}: {e}", cmd.get_program()))?;

    fn drain<R: std::io::Read + Send + 'static>(
        pipe: Option<R>,
    ) -> std::thread::JoinHandle<Vec<u8>> {
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            if let Some(mut p) = pipe {
                let _ = p.read_to_end(&mut buf);
            }
            buf
        })
    }
    let out_reader = drain(child.stdout.take());
    let err_reader = drain(child.stderr.take());

    let deadline = std::time::Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if std::time::Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "timed out after {}s — check network or proxy settings",
                    timeout.as_secs()
                ));
            }
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(100)),
            Err(e) => return Err(format!("Failed while waiting: {e}")),
        }
    };

    // The child has exited, so both pipes are at EOF and these finish promptly.
    let stdout = out_reader.join().unwrap_or_default();
    let stderr = err_reader.join().unwrap_or_default();

    Ok(std::process::Output {
        status,
        stdout,
        stderr,
    })
}

pub(crate) fn agents_cache_dir() -> PathBuf {
    #[cfg(feature = "desktop")]
    let base = directories::BaseDirs::new()
        .map(|d| d.data_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    #[cfg(not(feature = "desktop"))]
    let base = PathBuf::from(".");
    base.join("com.rotero.Rotero").join("agents")
}

pub(crate) fn find_mcp_binary() -> Option<PathBuf> {
    // `.exe` on Windows, empty elsewhere. Without it both filesystem probes
    // below silently miss on Windows and only the PATH lookup can succeed.
    let exe_name = format!("rotero-mcp{}", std::env::consts::EXE_SUFFIX);

    if let Ok(exe) = std::env::current_exe() {
        let sibling = exe.with_file_name(&exe_name);
        if sibling.exists() {
            return Some(sibling);
        }
    }

    // Dev-build fallback only: CARGO_MANIFEST_DIR is baked in at compile time,
    // so in a shipped bundle these paths point at the build machine.
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for dir in [
        &manifest_dir,
        &manifest_dir.join(".."),
        &manifest_dir.join("../.."),
    ] {
        for profile in ["release", "debug"] {
            let candidate = dir.join("target").join(profile).join(&exe_name);
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }

    which::which("rotero-mcp").ok()
}

/// Where to point a spawned child at PDFium, if anywhere.
///
/// The empty-string filter matters: `PDFIUM_DYNAMIC_LIB_PATH=` used to yield
/// `Some("")`, which the caller then passed to the child as an empty path — the
/// same trap that was fixed in the resolver itself.
pub(crate) fn find_pdfium_path() -> Option<PathBuf> {
    if let Some(p) = std::env::var("PDFIUM_DYNAMIC_LIB_PATH")
        .ok()
        .filter(|p| !p.is_empty())
    {
        return Some(PathBuf::from(p));
    }

    // Next to the executable, which is where a bundle stages it.
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
        && dir.join(pdfium_leafname()).exists()
    {
        return Some(dir.to_path_buf());
    }

    // Dev builds only: CARGO_MANIFEST_DIR is baked in at compile time, so in a
    // shipped binary it names a directory on the build machine.
    #[cfg(debug_assertions)]
    {
        let lib = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("lib");
        if lib.exists() {
            return Some(lib);
        }
    }

    None
}

/// The platform's PDFium library filename.
fn pdfium_leafname() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "libpdfium.dylib"
    }
    #[cfg(target_os = "windows")]
    {
        "pdfium.dll"
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        "libpdfium.so"
    }
}
