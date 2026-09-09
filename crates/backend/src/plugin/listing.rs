//! Projection of one cached registry entry into the marketplace summary the frontend renders.

use ora_contracts::{AvailablePlugin, PluginHostCompatibility};
use ora_plugin_asset::{AssetUrlForm, plugin_logo};
use ora_plugin_registry::RegistryEntry;

/// Converts one registry entry into the frontend-facing marketplace summary.
pub(super) fn available_plugin(entry: &RegistryEntry) -> AvailablePlugin {
    AvailablePlugin {
        id: entry.id().canonical(),
        name: entry.identifier().to_owned(),
        title: entry.title().to_owned(),
        kind: entry.kind().to_owned(),
        namespace: entry.namespace().to_owned(),
        source_url: entry.source_url().to_owned(),
        version: entry.version().to_string(),
        description: entry.description().to_owned(),
        logo: entry
            .logo()
            .and_then(|variants| plugin_logo(AssetUrlForm::CURRENT, entry.id(), &variants)),
        compatibility: match entry.host_compatibility() {
            Ok(()) => PluginHostCompatibility::Compatible,
            Err(reason) => PluginHostCompatibility::Incompatible { reason },
        },
    }
}
