//! Resolves one plugin id to the single directory its icon candidates may be read from.
//!
//! An icon exists in exactly two places, and a plugin is in at most one state: an installed
//! package has a package root, and a marketplace listing has an entry directory in the checkout
//! of the source that publishes it. Returning one directory rather than a list is what keeps the
//! protocol handler from searching: it either has a root for this id or it refuses.

use super::PluginApi;
use ora_domain::PluginId;
use ora_logging::ora_warn;
use ora_plugin_registry::RegistryIndex;
use std::path::PathBuf;

impl PluginApi {
    /// Returns the directory `plugin_id`'s icon candidates live in, if the host knows the plugin.
    ///
    /// The installed package wins over the marketplace entry, so an installed plugin is drawn
    /// from the bytes it actually runs from rather than from whatever its source publishes now.
    /// A source whose namespace differs from the id's cannot answer, so an id can never resolve
    /// into another source's checkout.
    ///
    /// A source that cannot be read is skipped with a warning: an icon must never be the reason
    /// a marketplace listing fails, and the caller treats an absent root exactly like an absent
    /// file.
    pub(crate) fn logo_directory(&self, plugin_id: &PluginId) -> Option<PathBuf> {
        if let Some(installed) = self.lifecycle.installed_plugin(plugin_id) {
            return Some(installed.package_root);
        }
        let sources = self
            .registry_sources()
            .inspect_err(|error| {
                ora_warn!(%error, "cannot resolve a plugin icon without marketplace sources");
            })
            .ok()?;
        for source in &sources {
            match RegistryIndex::resolve_entry_directory(source, plugin_id) {
                Ok(Some(entry_directory)) => return Some(entry_directory),
                Ok(None) => {}
                Err(error) => {
                    ora_warn!(%error, "skipping a marketplace source while resolving a plugin icon");
                }
            }
        }
        None
    }
}
