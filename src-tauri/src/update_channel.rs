//! Release channel identity (stable / beta / nightly) — build-time baked.
//!
//! The channel is a property of the installed build, derived from the version
//! string (`env!("CARGO_PKG_VERSION")`): prerelease containing `nightly` →
//! Nightly, `beta` → Beta, otherwise Stable. Users pick a channel by installing
//! that channel's build (Chrome/VS Code model); there is no runtime switch.
//! See docs/superpowers/specs/2026-07-31-update-channels-design.md (AC-10.9, D1).

/// Release channel this build tracks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateChannel {
    Stable,
    Beta,
    Nightly,
}

impl UpdateChannel {
    /// Lowercase channel id for DTOs / logs.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Stable => "stable",
            Self::Beta => "beta",
            Self::Nightly => "nightly",
        }
    }

    /// Derive the channel from a version string (`1.0.0`, `1.1.0-beta.1`,
    /// `0.3.1-nightly`, `1.1.0-nightly.20260801`; optional leading `v`).
    /// Unparseable versions fall back to Stable (most conservative feed).
    pub fn from_version(version: &str) -> Self {
        let v = version.trim().trim_start_matches(['v', 'V']);
        let Ok(parsed) = semver::Version::parse(v) else {
            return Self::Stable;
        };
        let pre = parsed.pre.as_str().to_ascii_lowercase();
        if pre.contains("nightly") {
            Self::Nightly
        } else if pre.contains("beta") {
            Self::Beta
        } else {
            Self::Stable
        }
    }

    /// True when a release tag belongs to this channel.
    pub fn owns_tag(self, tag: &str) -> bool {
        Self::from_version(tag) == self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_version_maps_prerelease_channels() {
        assert_eq!(UpdateChannel::from_version("1.0.0"), UpdateChannel::Stable);
        assert_eq!(UpdateChannel::from_version("v1.0.0"), UpdateChannel::Stable);
        assert_eq!(UpdateChannel::from_version("1.1.0-beta.1"), UpdateChannel::Beta);
        assert_eq!(UpdateChannel::from_version("v1.1.0-beta.2"), UpdateChannel::Beta);
        assert_eq!(UpdateChannel::from_version("0.3.1-nightly"), UpdateChannel::Nightly);
        assert_eq!(
            UpdateChannel::from_version("1.1.0-nightly.20260801"),
            UpdateChannel::Nightly
        );
        assert_eq!(UpdateChannel::from_version("0.0.0"), UpdateChannel::Stable);
    }

    #[test]
    fn from_version_falls_back_to_stable_on_garbage() {
        assert_eq!(UpdateChannel::from_version(""), UpdateChannel::Stable);
        assert_eq!(UpdateChannel::from_version("nope"), UpdateChannel::Stable);
        assert_eq!(UpdateChannel::from_version("1.0"), UpdateChannel::Stable);
    }

    #[test]
    fn as_str_lowercase_ids() {
        assert_eq!(UpdateChannel::Stable.as_str(), "stable");
        assert_eq!(UpdateChannel::Beta.as_str(), "beta");
        assert_eq!(UpdateChannel::Nightly.as_str(), "nightly");
    }

    #[test]
    fn owns_tag_matches_channel_membership() {
        assert!(UpdateChannel::Nightly.owns_tag("v0.3.1-nightly"));
        assert!(!UpdateChannel::Nightly.owns_tag("v1.0.0"));
        assert!(UpdateChannel::Stable.owns_tag("1.0.0"));
        assert!(!UpdateChannel::Stable.owns_tag("1.1.0-beta.1"));
        assert!(UpdateChannel::Beta.owns_tag("v1.1.0-beta.1"));
    }
}
