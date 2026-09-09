//! What a directory's icon files resolve to: either one icon for both themes, or a pair.

use super::candidate::{LogoExtension, LogoRole};
use serde::{Deserialize, Serialize};

/// The one candidate file a resolved half of an icon composition is served from.
///
/// Both halves of the pair are addressed this way rather than by theme alone, because the role
/// that *labels* a half and the role whose file *backs* it are not always the same: a plugin
/// that ships `logo.dark.svg` beside `logo.svg` has its light half backed by the universal file.
/// The URL has to name the file that exists, so the role travels with the extension.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LogoCandidate {
    pub role: LogoRole,
    pub extension: LogoExtension,
}

/// The icon composition one plugin publishes.
///
/// Modelling the two shapes as an enum rather than a struct of optional fields is what keeps a
/// half-built pair — a light icon with no dark one — unrepresentable, so the renderer branches
/// on the variant and never has to decide anything itself.
///
/// Nothing here records a filename or a path: a candidate is a role plus an extension, and the
/// host rebuilds `logo` + optional role + extension from those two closed sets whenever it needs
/// the name. Storing whole filenames would make values outside the fifteen candidates
/// expressible, and storing paths would put back exactly what the convention exists to keep out.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub enum PluginLogoVariants {
    /// One file serves both themes.
    Universal { universal: LogoCandidate },
    /// A theme pair, whose halves may come from different files and different formats.
    Themed {
        light: LogoCandidate,
        dark: LogoCandidate,
    },
}

impl PluginLogoVariants {
    /// Folds the three roles that may exist on disk into the composition they describe.
    ///
    /// All eight combinations are defined, and none of them is an error: an author who ships one
    /// file too few gets a sensible icon rather than a diagnostic they cannot act on.
    ///
    /// A theme pair wins whenever both halves exist, which incidentally keeps an author who
    /// ships all three files readable by an older host that only knows `logo.svg`. A single
    /// themed file is completed by the universal one when there is one to complete it with —
    /// shipping a dark icon says the unmarked file was drawn for light backgrounds — and falls
    /// back to serving that lone file under both themes when there is not.
    pub fn from_roles(
        light: Option<LogoExtension>,
        dark: Option<LogoExtension>,
        universal: Option<LogoExtension>,
    ) -> Option<Self> {
        let candidate =
            |role: LogoRole, extension: LogoExtension| LogoCandidate { role, extension };
        let light = light.map(|extension| candidate(LogoRole::Light, extension));
        let dark = dark.map(|extension| candidate(LogoRole::Dark, extension));
        let universal = universal.map(|extension| candidate(LogoRole::Universal, extension));
        match (light, dark, universal) {
            (Some(light), Some(dark), _) => Some(Self::Themed { light, dark }),
            (Some(light), None, Some(universal)) => Some(Self::Themed {
                light,
                dark: universal,
            }),
            (None, Some(dark), Some(universal)) => Some(Self::Themed {
                light: universal,
                dark,
            }),
            (Some(only), None, None) | (None, Some(only), None) => {
                Some(Self::Universal { universal: only })
            }
            (None, None, Some(universal)) => Some(Self::Universal { universal }),
            (None, None, None) => None,
        }
    }
}
