//! The one implementation of the candidate scan, shared by index building and package discovery.
//!
//! Both consumers face the same directory shape and the same rules, and keeping a single scan is
//! what stops the extension priority or the pair-completion rules from drifting apart between
//! them — a drift whose only symptom would be a plugin showing one icon in the marketplace and a
//! different one after install, which nobody would think to test for.

use super::candidate::{LOGO_EXTENSION_PRIORITY, LogoExtension, LogoRole, candidate_file_name};
use super::read::accepts_candidate;
use super::variants::PluginLogoVariants;
use ora_logging::ora_warn;
use std::path::Path;

/// Resolves the icon published in `directory` into its theme variants.
///
/// The scan is read-only: a package directory is byte-for-byte the unpacked archive and a
/// registry entry directory is an untrusted checkout, so neither is ever written to, not even to
/// cache what was found.
pub fn resolve_logo(directory: &Path) -> Option<PluginLogoVariants> {
    PluginLogoVariants::from_roles(
        resolve_role(directory, LogoRole::Light),
        resolve_role(directory, LogoRole::Dark),
        resolve_role(directory, LogoRole::Universal),
    )
}

/// Returns the winning extension for one role, or `None` when the role has no usable file.
///
/// The extensions are tried in a fixed order rather than in whatever order the directory lists
/// them, so the result of a scan depends only on which files exist. A candidate that exists but
/// cannot be served is skipped exactly like one that is absent, which is what lets a damaged
/// `logo.dark.webp` fall through to a sound `logo.dark.png` instead of costing the plugin its
/// dark icon — or its icon altogether.
fn resolve_role(directory: &Path, role: LogoRole) -> Option<LogoExtension> {
    for extension in LOGO_EXTENSION_PRIORITY {
        let path = directory.join(candidate_file_name(role, extension));
        match accepts_candidate(&path, extension) {
            Ok(()) => return Some(extension),
            Err(rejection) if rejection.is_absent() => {}
            Err(rejection) => {
                ora_warn!(
                    path = %path.display(),
                    error = %rejection,
                    "ignoring unusable plugin logo candidate"
                );
            }
        }
    }
    None
}
