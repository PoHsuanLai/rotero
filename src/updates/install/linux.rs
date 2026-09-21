//! Portable tarball install used by the in-app updater on Linux.
//!
//! Layout is a contract with `scripts/stage-linux-tarball.sh` and
//! `scripts/install-linux.sh`:
//!
//! ```text
//! rotero
//! *.so                         optional, next to the binary ($ORIGIN)
//! share/applications/rotero.desktop
//! share/icons/hicolor/*/apps/rotero.png
//! ```
//!
//! Dev builds under `target/` and system packages under `/usr` are refused —
//! those are not the copy the updater is allowed to replace.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use crate::updates::exe::running_exe;
use crate::updates::types::UpdateError;

pub(crate) fn install_update(bytes: &[u8]) -> Result<(), UpdateError> {
    let exe = running_exe()
        .ok_or_else(|| UpdateError::NotInstalled("Can't locate this executable.".into()))?;
    refuse_in_place_update(&exe)?;

    let tmp_dir = tempfile::tempdir()
        .map_err(|e| UpdateError::Install(format!("Failed to create temp dir: {e}")))?;
    extract_tar_gz(bytes, tmp_dir.path())?;
    let payload = payload_root(tmp_dir.path())?;
    apply_linux_payload(&payload, &exe)?;

    tracing::info!("Update installed to {}", exe.display());
    Ok(())
}

/// `cargo run` / `dx serve` land under `target/`; a package-manager install
/// lands under `/usr` or `/opt`. Neither is a copy we can swap in place.
fn refuse_in_place_update(exe: &Path) -> Result<(), UpdateError> {
    if is_dev_build(exe) {
        return Err(UpdateError::NotInstalled(
            "This copy is a development build. Install the desktop app \
             (scripts/install-linux.sh) to receive in-app updates."
                .into(),
        ));
    }
    if is_system_install(exe) {
        return Err(UpdateError::NotInstalled(
            "This copy was installed system-wide. Update it with your \
             package manager, or install the portable build to ~/.local."
                .into(),
        ));
    }
    Ok(())
}

fn is_dev_build(exe: &Path) -> bool {
    let components: Vec<_> = exe.iter().collect();
    components.windows(2).any(|pair| {
        pair[0] == "target" && matches!(pair[1].to_str(), Some("dx" | "debug" | "release"))
    })
}

fn is_system_install(exe: &Path) -> bool {
    exe.starts_with("/usr") || exe.starts_with("/opt")
}

/// Unpack a gzipped tarball, skipping `..` and absolute members.
fn extract_tar_gz(bytes: &[u8], dest: &Path) -> Result<(), UpdateError> {
    let decoder = flate2::read::GzDecoder::new(bytes);
    let mut archive = tar::Archive::new(decoder);
    let entries = archive
        .entries()
        .map_err(|e| UpdateError::Install(format!("Bad update archive: {e}")))?;
    for entry in entries {
        let mut entry =
            entry.map_err(|e| UpdateError::Install(format!("Bad update archive: {e}")))?;
        let path = entry
            .path()
            .map_err(|e| UpdateError::Install(format!("Bad update archive: {e}")))?
            .into_owned();
        if path
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
            || path.is_absolute()
        {
            continue;
        }
        let out = dest.join(&path);
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                UpdateError::Install(format!("Failed to create {}: {e}", parent.display()))
            })?;
        }
        entry.unpack(&out).map_err(|e| {
            UpdateError::Install(format!("Failed to extract {}: {e}", path.display()))
        })?;
    }
    Ok(())
}

/// Directory that actually contains `rotero` — the tarball is flat, but a
/// wrapping folder from a hand-packed archive is tolerated.
fn payload_root(extracted: &Path) -> Result<PathBuf, UpdateError> {
    let direct = extracted.join("rotero");
    if direct.is_file() {
        return Ok(extracted.to_path_buf());
    }
    if let Ok(entries) = std::fs::read_dir(extracted) {
        for entry in entries.flatten() {
            let candidate = entry.path().join("rotero");
            if entry.path().is_dir() && candidate.is_file() {
                return Ok(entry.path());
            }
        }
    }
    Err(UpdateError::Install(
        "No `rotero` binary inside the downloaded release".into(),
    ))
}

/// Swap the binary, copy `.so` sidecars next to it, and refresh the XDG
/// desktop file and icons when the tarball carries a `share/` tree.
fn apply_linux_payload(payload: &Path, exe: &Path) -> Result<(), UpdateError> {
    let staged = payload.join("rotero");
    let mut perms = std::fs::metadata(&staged)
        .map_err(|e| UpdateError::Install(format!("Failed to read staged binary: {e}")))?
        .permissions();
    perms.set_mode(0o755);
    let _ = std::fs::set_permissions(&staged, perms);

    self_replace::self_replace(&staged)
        .map_err(|e| UpdateError::Install(format!("Failed to replace the executable: {e}")))?;

    if let Some(dest_dir) = exe.parent() {
        copy_sidecars(payload, dest_dir)?;
    }
    install_xdg_share(payload, exe)?;
    Ok(())
}

fn copy_sidecars(payload: &Path, dest_dir: &Path) -> Result<(), UpdateError> {
    let entries = std::fs::read_dir(payload)
        .map_err(|e| UpdateError::Install(format!("Failed to read update payload: {e}")))?;
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name() else {
            continue;
        };
        if name == "rotero" || path.is_dir() {
            continue;
        }
        if path.extension().and_then(|e| e.to_str()) != Some("so") {
            continue;
        }
        std::fs::copy(&path, dest_dir.join(name)).map_err(|e| {
            UpdateError::Install(format!("Failed to install {}: {e}", path.display()))
        })?;
    }
    Ok(())
}

/// Refresh `~/.local/share/{applications,icons}` from the tarball's `share/`.
///
/// Skipped for system installs so a user-local `.desktop` does not shadow the
/// package-manager copy. Missing `share/` (older tarballs) is a no-op: the
/// binary still updates.
fn install_xdg_share(payload: &Path, exe: &Path) -> Result<(), UpdateError> {
    if is_system_install(exe) {
        return Ok(());
    }
    let share = payload.join("share");
    if !share.is_dir() {
        return Ok(());
    }
    let Some(data_home) = user_data_home() else {
        return Ok(());
    };
    install_xdg_share_into(&share, &data_home, exe)?;
    refresh_desktop_caches(&data_home);
    Ok(())
}

fn user_data_home() -> Option<PathBuf> {
    match std::env::var_os("XDG_DATA_HOME") {
        Some(val) if !val.is_empty() => Some(PathBuf::from(val)),
        _ => std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")),
    }
}

fn install_xdg_share_into(share: &Path, data_home: &Path, exe: &Path) -> Result<(), UpdateError> {
    let desktop_src = share.join("applications/rotero.desktop");
    if desktop_src.is_file() {
        let dest_dir = data_home.join("applications");
        std::fs::create_dir_all(&dest_dir).map_err(|e| {
            UpdateError::Install(format!("Failed to create {}: {e}", dest_dir.display()))
        })?;
        let contents = std::fs::read_to_string(&desktop_src)
            .map_err(|e| UpdateError::Install(format!("Failed to read desktop file: {e}")))?;
        let rewritten = rewrite_desktop_exec(&contents, exe);
        std::fs::write(dest_dir.join("rotero.desktop"), rewritten)
            .map_err(|e| UpdateError::Install(format!("Failed to write desktop file: {e}")))?;
    }

    let icons_src = share.join("icons");
    if icons_src.is_dir() {
        copy_dir_all(&icons_src, &data_home.join("icons"))?;
    }
    Ok(())
}

fn rewrite_desktop_exec(contents: &str, exe: &Path) -> String {
    let exe = exe.display().to_string();
    let mut out = String::with_capacity(contents.len() + exe.len());
    for line in contents.lines() {
        if let Some(rest) = line.strip_prefix("Exec=") {
            out.push_str("Exec=");
            out.push_str(&exe);
            if let Some((_, args)) = rest.split_once(' ') {
                out.push(' ');
                out.push_str(args);
            }
            out.push('\n');
        } else if line.starts_with("TryExec=") {
            out.push_str("TryExec=");
            out.push_str(&exe);
            out.push('\n');
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

fn copy_dir_all(src: &Path, dest: &Path) -> Result<(), UpdateError> {
    std::fs::create_dir_all(dest)
        .map_err(|e| UpdateError::Install(format!("Failed to create {}: {e}", dest.display())))?;
    let entries = std::fs::read_dir(src)
        .map_err(|e| UpdateError::Install(format!("Failed to read {}: {e}", src.display())))?;
    for entry in entries.flatten() {
        let from = entry.path();
        let to = dest.join(entry.file_name());
        if from.is_dir() {
            copy_dir_all(&from, &to)?;
        } else {
            if let Some(parent) = to.parent() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    UpdateError::Install(format!("Failed to create {}: {e}", parent.display()))
                })?;
            }
            std::fs::copy(&from, &to).map_err(|e| {
                UpdateError::Install(format!("Failed to copy {}: {e}", from.display()))
            })?;
        }
    }
    Ok(())
}

fn refresh_desktop_caches(data_home: &Path) {
    let applications = data_home.join("applications");
    let _ = std::process::Command::new("update-desktop-database")
        .arg(&applications)
        .status();
    let hicolor = data_home.join("icons/hicolor");
    if hicolor.is_dir() {
        let _ = std::process::Command::new("gtk-update-icon-cache")
            .args(["-f", "-t"])
            .arg(&hicolor)
            .status();
    }
    let _ = std::process::Command::new("kbuildsycoca6")
        .arg("--noincremental")
        .status();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn make_tarball(files: &[(&str, &[u8])]) -> Vec<u8> {
        use flate2::Compression;
        use flate2::write::GzEncoder;

        let encoder = GzEncoder::new(Vec::new(), Compression::default());
        let mut builder = tar::Builder::new(encoder);
        for (name, data) in files {
            let mut header = tar::Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_mode(if *name == "rotero" { 0o755 } else { 0o644 });
            header.set_cksum();
            builder.append_data(&mut header, name, *data).unwrap();
        }
        builder.into_inner().unwrap().finish().unwrap()
    }

    #[test]
    fn shipped_desktop_file_rewrites_exec_to_the_install_path() {
        let src = include_str!("../../../packaging/linux/rotero.desktop");
        assert!(src.contains("Name=Rotero"));
        assert!(src.contains("Exec=rotero"));
        assert!(src.contains("Icon=rotero"));
        assert!(src.contains("StartupWMClass=rotero"));

        let rewritten = rewrite_desktop_exec(src, Path::new("/home/me/.local/lib/rotero/rotero"));
        assert!(rewritten.contains("Exec=/home/me/.local/lib/rotero/rotero\n"));
        assert!(rewritten.contains("TryExec=/home/me/.local/lib/rotero/rotero\n"));
        assert!(!rewritten.contains("Exec=rotero\n"));
    }

    #[test]
    fn rewrite_desktop_exec_keeps_arguments() {
        let src = "Exec=rotero --foo\nTryExec=rotero\n";
        let out = rewrite_desktop_exec(src, Path::new("/opt/rotero"));
        assert_eq!(out, "Exec=/opt/rotero --foo\nTryExec=/opt/rotero\n");
    }

    #[test]
    fn dev_and_system_paths_are_refused() {
        assert!(is_dev_build(Path::new(
            "/home/me/rotero/target/dx/rotero/release/linux/rotero"
        )));
        assert!(is_dev_build(Path::new(
            "/home/me/rotero/target/release/rotero"
        )));
        assert!(!is_dev_build(Path::new(
            "/home/me/.local/lib/rotero/rotero"
        )));
        assert!(is_system_install(Path::new("/usr/bin/rotero")));
        assert!(is_system_install(Path::new("/opt/rotero/bin/rotero")));
        assert!(!is_system_install(Path::new(
            "/home/me/.local/lib/rotero/rotero"
        )));
        assert!(refuse_in_place_update(Path::new("/usr/bin/rotero")).is_err());
        assert!(refuse_in_place_update(Path::new("/home/me/.local/lib/rotero/rotero")).is_ok());
    }

    #[test]
    fn extract_tarball_installs_desktop_and_sidecars_into_xdg() {
        let bytes = make_tarball(&[
            ("rotero", b"fake-binary"),
            ("libpdfium.so", b"fake-so"),
            (
                "share/applications/rotero.desktop",
                b"[Desktop Entry]\nName=Rotero\nExec=rotero\nTryExec=rotero\n",
            ),
            ("share/icons/hicolor/48x48/apps/rotero.png", b"png"),
        ]);

        let extracted = tempfile::tempdir().unwrap();
        extract_tar_gz(&bytes, extracted.path()).unwrap();
        let payload = payload_root(extracted.path()).unwrap();
        assert!(payload.join("rotero").is_file());
        assert!(payload.join("libpdfium.so").is_file());

        let dest = tempfile::tempdir().unwrap();
        copy_sidecars(&payload, dest.path()).unwrap();
        assert!(dest.path().join("libpdfium.so").is_file());
        assert!(!dest.path().join("rotero").exists());

        let data_home = tempfile::tempdir().unwrap();
        let exe = dest.path().join("rotero");
        install_xdg_share_into(&payload.join("share"), data_home.path(), &exe).unwrap();
        let desktop =
            std::fs::read_to_string(data_home.path().join("applications/rotero.desktop")).unwrap();
        assert!(desktop.contains(&format!("Exec={}", exe.display())));
        assert!(
            data_home
                .path()
                .join("icons/hicolor/48x48/apps/rotero.png")
                .is_file()
        );
    }

    #[test]
    fn payload_root_accepts_a_wrapping_directory() {
        let bytes = make_tarball(&[("Rotero/rotero", b"fake-binary")]);
        let extracted = tempfile::tempdir().unwrap();
        extract_tar_gz(&bytes, extracted.path()).unwrap();
        let payload = payload_root(extracted.path()).unwrap();
        assert!(payload.ends_with("Rotero"));
        assert!(payload.join("rotero").is_file());
    }
}
