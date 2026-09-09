//! The asset URLs an icon is served under, and the request shape the protocol handler parses.
//!
//! An icon request has no surface instance to be authorised against: the main window issues it
//! for an arbitrary plugin, and a marketplace listing that is not installed has no instance at
//! all. The URL therefore names three things and nothing else — plugin id, theme role, file
//! extension — so that the handler can rebuild the candidate filename from closed sets rather
//! than trusting any path the caller supplies.

use super::candidate::{LogoExtension, LogoRole};
use super::variants::{LogoCandidate, PluginLogoVariants};
use crate::scheme::AssetUrlForm;
use ora_contracts::PluginLogo;
use ora_domain::PluginId;
use url::{ParseError, Url};

/// First path segment of every icon request, which separates the branch from instance assets.
///
/// A surface instance is addressed by a number, so a reserved word can never collide with one
/// and the handler can pick the authorisation chain from the first segment alone.
pub const LOGO_URL_PREFIX: &str = "logo";

/// One icon request as addressed by its URL path.
///
/// Holding a parsed [`PluginId`] and the two closed-set values is the whole point of the type:
/// once a request exists, every part of the filename it will resolve to has already been checked
/// against a fixed set, and no caller-supplied string survives into path construction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LogoAssetRequest {
    pub plugin_id: PluginId,
    pub role: LogoRole,
    pub extension: LogoExtension,
}

impl LogoAssetRequest {
    /// Parses `/logo/<namespace>/<name>/<role>.<extension>` into its three closed-set parts.
    ///
    /// `path` must already be percent-decoded, exactly once, by the caller. Decoding here as
    /// well would let an encoded separator survive the split and reappear afterwards; leaving it
    /// undone would instead make a legitimately encoded segment fail the id grammar. Neither
    /// mistake can produce a traversal, because every segment is then checked against a closed
    /// set: the id grammar admits only lowercase letters, digits, `-` and `.` and refuses `.`
    /// and `..`, while the role and extension must equal one of the eight fixed spellings.
    pub fn parse(path: &str) -> Option<Self> {
        let mut segments = path.trim_start_matches('/').split('/');
        if segments.next()? != LOGO_URL_PREFIX {
            return None;
        }
        let namespace = segments.next()?;
        let name = segments.next()?;
        let file_name = segments.next()?;
        if segments.next().is_some() {
            return None;
        }
        let (role, extension) = file_name.split_once('.')?;
        Some(Self {
            plugin_id: PluginId::new(namespace, name).ok()?,
            role: LogoRole::parse(role)?,
            extension: LogoExtension::parse(extension)?,
        })
    }
}

/// Returns the URL one plugin's icon candidate is served from on this host.
pub fn logo_asset_url(
    form: AssetUrlForm,
    plugin_id: &PluginId,
    role: LogoRole,
    extension: LogoExtension,
) -> Result<Url, ParseError> {
    Url::parse(&format!(
        "{origin}{LOGO_URL_PREFIX}/{namespace}/{name}/{role}.{extension}",
        origin = form.origin(),
        namespace = plugin_id.namespace(),
        name = plugin_id.name(),
        role = role.as_str(),
        extension = extension.as_str(),
    ))
}

/// Turns a resolved icon composition into the contract shape the frontend renders.
///
/// The contract carries URLs rather than content: the bytes are fetched only for the icons that
/// are actually drawn, they belong to the webview's image cache instead of a JS string that
/// lives as long as the list, and the format stops mattering to the contract entirely, which is
/// what makes bitmap icons possible at all.
pub fn plugin_logo(
    form: AssetUrlForm,
    plugin_id: &PluginId,
    variants: &PluginLogoVariants,
) -> Option<PluginLogo> {
    let url = |candidate: LogoCandidate| {
        logo_asset_url(form, plugin_id, candidate.role, candidate.extension)
            .ok()
            .map(String::from)
    };
    match *variants {
        PluginLogoVariants::Universal { universal } => Some(PluginLogo::Universal {
            url: url(universal)?,
        }),
        PluginLogoVariants::Themed { light, dark } => Some(PluginLogo::Themed {
            light: url(light)?,
            dark: url(dark)?,
        }),
    }
}
