//! Zip install used by the in-app updater on Windows (and other non-macOS,
//! non-Linux targets): extract the executable and `self-replace` it.

use crate::updates::exe::running_exe;
use crate::updates::types::UpdateError;

/// Replaces the running executable in place.
///
/// `self-replace` handles the part Windows makes hard: an image that is
/// currently executing can't simply be overwritten, so it has to be renamed
/// aside and cleaned up afterwards.
pub(crate) fn install_update(bytes: &[u8]) -> Result<(), UpdateError> {
    let exe = running_exe()
        .ok_or_else(|| UpdateError::NotInstalled("Can't locate this executable.".into()))?;
    let exe_name = exe
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "rotero".into());

    let tmp_dir = tempfile::tempdir()
        .map_err(|e| UpdateError::Install(format!("Failed to create temp dir: {e}")))?;
    let staged = extract_executable(bytes, tmp_dir.path(), &exe_name)?;

    self_replace::self_replace(&staged)
        .map_err(|e| UpdateError::Install(format!("Failed to replace the executable: {e}")))?;

    tracing::info!("Update installed to {}", exe.display());
    Ok(())
}

/// Pulls the new executable out of the release zip into `dir`.
fn extract_executable(
    bytes: &[u8],
    dir: &std::path::Path,
    exe_name: &str,
) -> Result<std::path::PathBuf, UpdateError> {
    let out = dir.join(exe_name);
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes))
        .map_err(|e| UpdateError::Install(format!("Bad update archive: {e}")))?;
    for i in 0..archive.len() {
        let mut file = archive
            .by_index(i)
            .map_err(|e| UpdateError::Install(format!("Bad update archive: {e}")))?;
        let Some(name) = file.enclosed_name() else {
            continue;
        };
        if name.file_name().is_some_and(|n| n == exe_name) {
            let mut sink = std::fs::File::create(&out)
                .map_err(|e| UpdateError::Install(format!("Failed to stage the update: {e}")))?;
            std::io::copy(&mut file, &mut sink)
                .map_err(|e| UpdateError::Install(format!("Failed to stage the update: {e}")))?;
            return Ok(out);
        }
    }

    Err(UpdateError::Install(format!(
        "No `{exe_name}` inside the downloaded release"
    )))
}
