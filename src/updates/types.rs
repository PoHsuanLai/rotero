/// GitHub release the updater can install.
#[derive(Debug, Clone, Default)]
pub struct UpdateInfo {
    pub latest_version: String,
    pub release_notes: String,
    pub download_url: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum UpdateStatus {
    #[default]
    Idle,
    Checking,
    Available,
    Downloading,
    ReadyToRestart,
    UpToDate,
    Error,
}

#[derive(Debug, Clone, Default)]
pub struct UpdateState {
    pub status: UpdateStatus,
    pub info: Option<UpdateInfo>,
    pub error: Option<UpdateError>,
    pub show_dialog: bool,
}

/// Why an update check or install failed.
///
/// Typed rather than a bare `String` so the dialog can say something useful —
/// the raw `reqwest`/`io` text was previously interpolated straight into the UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateError {
    /// Couldn't reach GitHub, or it returned something unusable.
    Network(String),
    /// A release exists, but nothing in it matches this platform.
    NoAssetForPlatform,
    /// This build can't replace itself in place (dev build, odd layout).
    NotInstalled(String),
    /// The download or swap itself failed.
    Install(String),
}

impl UpdateError {
    /// One-line summary for the dialog heading.
    pub fn headline(&self) -> &'static str {
        match self {
            Self::Network(_) => "Couldn't reach GitHub",
            Self::NoAssetForPlatform => "No download for this platform",
            Self::NotInstalled(_) => "Can't update this build",
            Self::Install(_) => "Couldn't install the update",
        }
    }

    /// What the user can actually do about it.
    pub fn guidance(&self) -> String {
        match self {
            Self::Network(detail) => {
                format!("Check your connection and try again.\n\n{detail}")
            }
            Self::NoAssetForPlatform => format!(
                "This release has no build for {} {}. Download it from the \
                 releases page instead.",
                std::env::consts::OS,
                std::env::consts::ARCH,
            ),
            Self::NotInstalled(detail) => format!(
                "Updating in place only works for an installed copy. \
                 Download the latest release manually.\n\n{detail}"
            ),
            Self::Install(detail) => {
                format!("Download the latest release manually.\n\n{detail}")
            }
        }
    }
}

impl std::fmt::Display for UpdateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.headline(), self.guidance())
    }
}
