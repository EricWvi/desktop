//! The icon branch of the `ora-plugin://` protocol: serves one plugin's brand mark to the main
//! window, for any plugin the host knows — installed or only listed in a marketplace checkout.
//!
//! It shares the protocol shell with workbench assets and nothing else. A workbench request is
//! authorised by resolving the caller's label to a live surface instance, and an icon request has
//! no instance to resolve: it is issued by the main window on behalf of an arbitrary plugin, and
//! a marketplace listing that is not installed never had an instance at all. So the two branches
//! are separate chains, and this one is deliberately the narrower of the two — one caller label,
//! two possible roots, and a filename the host builds itself out of closed sets.

use crate::surface::MAIN_WINDOW_LABEL;
use crate::surface::workbench_assets::AssetOutcome;
use ora_backend::Plugins;
use ora_plugin_asset::{LogoAssetRequest, asset_content_type, candidate_file_name};
use ora_utils::path::{CanonicalPathRoot, PortableRelativePath};

/// Resolves one icon request issued by the webview `label`.
///
/// Refusals are indistinguishable to the caller, as on the workbench branch; the reason only
/// reaches the log. The chain is: the caller must be the main window, the URL must parse into a
/// plugin id, theme role and extension that all come from closed sets, the host must know that
/// plugin, and the candidate filename the host builds from the role and extension must resolve
/// inside that plugin's own root.
pub fn resolve_logo_asset(plugins: &Plugins, label: &str, request_path: &str) -> AssetOutcome {
    // The only caller with a legitimate reason to ask for an arbitrary plugin's icon is the
    // trusted shell; a plugin's own workbench or webview has no business reaching this branch.
    if label != MAIN_WINDOW_LABEL {
        return AssetOutcome::NotFound("icons are served to the main window only");
    }
    // Decoded exactly once, before parsing. Doing it again afterwards would let an encoded
    // separator reappear inside a segment that has already been checked.
    let Ok(decoded) = urlencoding::decode(request_path) else {
        return AssetOutcome::NotFound("path is not valid UTF-8");
    };
    let Some(request) = LogoAssetRequest::parse(&decoded) else {
        return AssetOutcome::NotFound("path is not a plugin id, role and extension");
    };
    let Some(directory) = plugins.logo_directory(&request.plugin_id) else {
        return AssetOutcome::NotFound("no installed package or registry entry owns this id");
    };
    // The filename never comes from the URL: it is rebuilt from two closed sets, so the only
    // thing the request contributes to the path is a plugin id that passed the id grammar.
    let file_name = candidate_file_name(request.role, request.extension);
    let Ok(relative) = PortableRelativePath::parse(&file_name) else {
        return AssetOutcome::NotFound("candidate name is not a safe relative path");
    };
    let Ok(root) = CanonicalPathRoot::new(&directory) else {
        return AssetOutcome::NotFound("plugin icon root is unavailable");
    };
    let Ok(resolved) = root.resolve_existing(&relative) else {
        return AssetOutcome::NotFound("icon does not resolve inside the plugin root");
    };
    if !resolved.is_file() {
        return AssetOutcome::NotFound("icon path is not a regular file");
    }
    match std::fs::read(&resolved) {
        // The content type comes from the extension alone. It can, because the extension was
        // checked against the file's magic bytes when the icon was resolved, so the type always
        // describes the bytes without the response ever sniffing them or consulting the index.
        Ok(body) => AssetOutcome::Serve {
            content_type: asset_content_type(request.extension.as_str()),
            body,
            csp_base: None,
        },
        Err(_) => AssetOutcome::NotFound("icon could not be read"),
    }
}
