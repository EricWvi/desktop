//! Desktop plugin operations.

use super::run_async_backend;
use crate::{error::CommandError, state::DesktopState};
use ora_contracts::*;
use serde::Serialize;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use tauri::{AppHandle, Emitter, State};

const PLUGIN_INSTALL_PROGRESS_EVENT: &str = "plugin-install-progress";
const PLUGIN_PROGRESS_EVENT_INTERVAL: u64 = 1024 * 1024;

/// Carries byte-level marketplace package progress to the plugin card that started the install.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PluginInstallProgressEvent {
    plugin_id: String,
    downloaded: u64,
    total: Option<u64>,
}

backend_command!(
    list_installed_plugins,
    ListInstalledPluginsRequest,
    ListInstalledPluginsResponse,
    list_installed_plugins,
    "Lists the cached installed-plugin lifecycle snapshot."
);
backend_command!(
    get_plugin_configuration,
    GetPluginConfigurationRequest,
    GetPluginConfigurationResponse,
    get_plugin_configuration,
    "Loads one typed Plugin Configuration editor snapshot."
);
backend_command!(
    save_plugin_configuration,
    SavePluginConfigurationRequest,
    SavePluginConfigurationResponse,
    save_plugin_configuration,
    "Persists one revision-checked Plugin Configuration replacement."
);
backend_command!(
    reset_plugin_configuration,
    ResetPluginConfigurationRequest,
    ResetPluginConfigurationResponse,
    reset_plugin_configuration,
    "Resets explicit overrides or recovers a damaged Plugin Configuration."
);
backend_command!(
    list_available_plugins,
    ListAvailablePluginsRequest,
    ListAvailablePluginsResponse,
    list_available_plugins,
    "Lists the cached marketplace registry index."
);
backend_command!(
    sync_available_plugins,
    SyncAvailablePluginsRequest,
    SyncAvailablePluginsResponse,
    sync_available_plugins,
    "Pulls the marketplace source and rebuilds the cached registry index."
);
backend_command!(
    read_plugin_readme,
    ReadPluginReadmeRequest,
    ReadPluginReadmeResponse,
    read_plugin_readme,
    "Reads one marketplace plugin's published README for its detail page."
);

backend_command!(
    list_marketplace_sources,
    ListMarketplaceSourcesRequest,
    ListMarketplaceSourcesResponse,
    list_marketplace_sources,
    "Lists the configured marketplace source repositories."
);
backend_command!(
    add_marketplace_source,
    AddMarketplaceSourceRequest,
    AddMarketplaceSourceResponse,
    add_marketplace_source,
    "Adds one marketplace source repository."
);
backend_command!(
    delete_marketplace_source,
    DeleteMarketplaceSourceRequest,
    DeleteMarketplaceSourceResponse,
    delete_marketplace_source,
    "Removes one marketplace source repository."
);
backend_command!(
    update_marketplace_source,
    UpdateMarketplaceSourceRequest,
    UpdateMarketplaceSourceResponse,
    update_marketplace_source,
    "Updates one marketplace source's URL, branch, proxy policy, or enabled state."
);
async_backend_command!(
    scan_plugins,
    ScanPluginsRequest,
    ScanPluginsResponse,
    scan_plugins,
    "Explicitly scans and reconciles installed plugins."
);
async_backend_command!(
    activate_plugin,
    ActivatePluginRequest,
    ActivatePluginResponse,
    activate_plugin,
    "Activates one installed plugin."
);
async_backend_command!(
    stop_plugin,
    StopPluginRequest,
    StopPluginResponse,
    stop_plugin,
    "Stops one plugin process."
);
async_backend_command!(
    uninstall_plugin,
    UninstallPluginRequest,
    UninstallPluginResponse,
    uninstall_plugin,
    "Stops and removes one installed plugin."
);
/// Installs one marketplace plugin and emits throttled byte-level download progress.
#[tauri::command]
pub async fn install_plugin(
    state: State<'_, DesktopState>,
    app: AppHandle,
    request: InstallPluginRequest,
) -> Result<InstallPluginResponse, CommandError> {
    let plugin_id = request.plugin_id.clone();
    let last_emitted = Arc::new(AtomicU64::new(0));
    let progress = Arc::new(move |progress: ora_utils::http::Progress| {
        let previous = last_emitted.load(Ordering::Relaxed);
        let complete = progress.total == Some(progress.bytes);
        if previous != 0
            && progress.bytes < previous.saturating_add(PLUGIN_PROGRESS_EVENT_INTERVAL)
            && !complete
        {
            return;
        }
        last_emitted.store(progress.bytes, Ordering::Relaxed);
        let event = PluginInstallProgressEvent {
            plugin_id: plugin_id.clone(),
            downloaded: progress.bytes,
            total: progress.total,
        };
        if let Err(error) = app.emit(PLUGIN_INSTALL_PROGRESS_EVENT, event) {
            ora_logging::ora_warn!(message = "Failed to emit plugin install progress", error = %error);
        }
    });

    run_async_backend(
        "install_plugin",
        state
            .backend
            .install_plugin_with_progress(request, progress),
    )
    .await
}
async_backend_command!(
    update_plugin,
    UpdatePluginRequest,
    UpdatePluginResponse,
    update_plugin,
    "Updates one installed plugin to the version its marketplace source publishes."
);
async_backend_command!(
    import_plugin,
    ImportPluginRequest,
    ImportPluginResponse,
    import_plugin,
    "Imports one local .orax release archive; the installed plugin is immediately available."
);
