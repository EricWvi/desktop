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

#[cfg(test)]
mod tests {
    use super::super::variants::PluginLogoVariants;
    use super::{LogoAssetRequest, LogoExtension, LogoRole, logo_asset_url, plugin_logo};
    use crate::scheme::AssetUrlForm;
    use ora_contracts::PluginLogo;
    use ora_domain::PluginId;
    use pretty_assertions::assert_eq;

    /// The plugin every URL in this module is built for.
    fn plugin() -> PluginId {
        PluginId::new("official", "acme.hub").expect("plugin id")
    }

    /// A URL names the plugin id, the role and the extension, in both platform spellings.
    #[test]
    fn spells_the_icon_url_in_both_platform_forms() {
        let url = |form| {
            logo_asset_url(form, &plugin(), LogoRole::Dark, LogoExtension::Png)
                .expect("icon url")
                .to_string()
        };

        assert_eq!(
            (
                url(AssetUrlForm::CustomScheme),
                url(AssetUrlForm::HttpLocalhost)
            ),
            (
                "ora-plugin://localhost/logo/official/acme.hub/dark.png".to_owned(),
                "http://ora-plugin.localhost/logo/official/acme.hub/dark.png".to_owned(),
            )
        );
    }

    /// A well-formed request parses back into the three closed-set parts the URL carries.
    #[test]
    fn parses_a_well_formed_icon_request() {
        assert_eq!(
            LogoAssetRequest::parse("/logo/official/acme.hub/light.jpeg"),
            Some(LogoAssetRequest {
                plugin_id: plugin(),
                role: LogoRole::Light,
                extension: LogoExtension::Jpeg,
            })
        );
    }

    /// Every URL a request round-trips from is one the parser accepts again.
    #[test]
    fn round_trips_every_url_it_builds() {
        let url = logo_asset_url(
            AssetUrlForm::CustomScheme,
            &plugin(),
            LogoRole::Universal,
            LogoExtension::Webp,
        )
        .expect("icon url");

        assert_eq!(
            LogoAssetRequest::parse(url.path()),
            Some(LogoAssetRequest {
                plugin_id: plugin(),
                role: LogoRole::Universal,
                extension: LogoExtension::Webp,
            })
        );
    }

    /// Anything outside the three closed sets is refused before a filename is ever built.
    ///
    /// The refusals matter more than the acceptances here: this branch serves an installed
    /// package root and an untrusted checkout, so a request that could smuggle a path segment,
    /// a traversal, or an unlisted extension past the parser would be an arbitrary file read.
    #[test]
    fn refuses_everything_outside_the_closed_sets() {
        let refused = [
            // A role or extension that is not one of the fixed spellings.
            "/logo/official/acme.hub/themed.svg",
            "/logo/official/acme.hub/dark.gif",
            "/logo/official/acme.hub/dark.exe",
            // A path segment where the filename belongs, and a deeper path below it.
            "/logo/official/acme.hub/nested/dark.svg",
            "/logo/official/acme.hub/dark.svg/extra",
            // Traversal spelled directly, and spelled through the id segments.
            "/logo/official/../../secret/dark.svg",
            "/logo/../acme.hub/dark.svg",
            "/logo/official/./dark.svg",
            // An id segment outside the id grammar, uppercase and separators included.
            "/logo/Official/acme.hub/dark.svg",
            "/logo/official/acme hub/dark.svg",
            "/logo//acme.hub/dark.svg",
            // A request that is not an icon request at all.
            "/7/index.html",
            "/logo/official/acme.hub",
            "/logo",
            "",
        ];

        assert_eq!(
            refused.map(|path| LogoAssetRequest::parse(path).is_some()),
            [false; 15]
        );
    }

    /// A percent-encoded traversal does not survive, because it is never decoded a second time.
    ///
    /// The caller decodes exactly once before parsing; these paths arrive still encoded, and the
    /// id grammar refuses the `%` outright instead of letting a decode turn it into a separator.
    #[test]
    fn refuses_percent_encoded_traversal_without_decoding_it() {
        assert_eq!(
            (
                LogoAssetRequest::parse("/logo/official/%2e%2e%2f%2e%2e/dark.svg"),
                LogoAssetRequest::parse("/logo/official/acme.hub/dark%2e%2e%2fsvg"),
                LogoAssetRequest::parse("/logo/%2e%2e/acme.hub/dark.svg"),
            ),
            (None, None, None)
        );
    }

    /// Each composition maps onto the contract shape the frontend branches on.
    #[test]
    fn maps_both_compositions_onto_the_contract() {
        let universal = PluginLogoVariants::from_roles(None, None, Some(LogoExtension::Svg))
            .expect("a universal candidate resolves");
        let themed = PluginLogoVariants::from_roles(
            Some(LogoExtension::Svg),
            None,
            Some(LogoExtension::Png),
        )
        .expect("a themed pair resolves");

        assert_eq!(
            (
                plugin_logo(AssetUrlForm::CustomScheme, &plugin(), &universal),
                plugin_logo(AssetUrlForm::CustomScheme, &plugin(), &themed),
            ),
            (
                Some(PluginLogo::Universal {
                    url: "ora-plugin://localhost/logo/official/acme.hub/universal.svg".to_owned(),
                }),
                // The dark half is backed by `logo.png`, so it addresses the universal role.
                Some(PluginLogo::Themed {
                    light: "ora-plugin://localhost/logo/official/acme.hub/light.svg".to_owned(),
                    dark: "ora-plugin://localhost/logo/official/acme.hub/universal.png".to_owned(),
                }),
            )
        );
    }
}
