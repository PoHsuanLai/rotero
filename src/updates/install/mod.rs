//! Platform-specific install of a downloaded release archive.

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(all(not(target_os = "macos"), not(target_os = "linux")))]
mod windows;

#[cfg(target_os = "linux")]
pub(crate) use linux::install_update;
#[cfg(target_os = "macos")]
pub(crate) use macos::install_update;
#[cfg(all(not(target_os = "macos"), not(target_os = "linux")))]
pub(crate) use windows::install_update;
