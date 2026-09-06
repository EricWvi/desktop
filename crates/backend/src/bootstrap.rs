use crate::agent::AgentApi;
use crate::agent_runtime::{AgentRuntimeManager, AgentRuntimeSetup, SessionEventStream};
use crate::app_event::AppEventHub;
use crate::clock::SystemClock;
use crate::error::BackendError;
use crate::git_cleanup::KeyedResourceLocks;
use crate::plugin::PluginApi;
use crate::plugin_gateway::PluginGateway;
use crate::project::ProjectApi;
use crate::repository_work::spawn_repository_work;
use crate::session::SessionApi;
use crate::settings::Settings;
use crate::skill::SkillApi;
use crate::task::{TaskApi, TaskSetup};
use crate::workflow::WorkflowApi;
use crate::workflow::run::WorkflowRunApi;
use crate::workflow::run::{
    ConcreteWorkflowRunControl, ConcreteWorkflowRunEngine, build_workflow_run_engine,
};
use crate::workspace::WorkspaceApi;
use ora_application::{ApplicationError, Clock, EffectService, WorkflowRunEngineRepository};
use ora_contracts::*;
use ora_db::SqliteWorkflowRunEngineRepository;
use ora_db::{DatabaseBootstrapper, DatabaseLocation, RepositoryPool, default_migration_catalog};
use ora_logging::{ora_error, ora_warn};
use ora_scheduler::Scheduler;
use ora_utils::http::ProgressCallback;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use thiserror::Error;

/// Names the persistent paths required to construct the shared backend.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackendPaths {
    /// Tauri application data root containing SQLite, sessions, and formal Skills.
    pub app_data_directory: PathBuf,
    /// Ora home root (`~/.ora`) containing plugins and worktrees.
    pub home_directory: PathBuf,
    /// Bundled Deno executable used for plugin activation.
    pub deno_path: PathBuf,
    /// Directory against which persisted relative local Workspace locations are resolved.
    ///
    /// Relative locations are stored against the directory from which `ORA_DATA_DIR`
    /// was created. Live process cwd is not used: Desktop `tauri dev` starts in
    /// `src-tauri`, which is not that directory.
    pub relative_path_base: PathBuf,
    /// IANA timezone used by backend-owned cron and delayed work.
    pub timezone: chrono_tz::Tz,
}

/// Reports failures that prevent the shared backend from opening persistent state.
#[derive(Debug, Error)]
pub enum BackendBootstrapError {
    #[error("failed to create backend directory {path:?}")]
    DirectoryCreate {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to bootstrap backend database")]
    Database(#[source] ora_db::DatabaseError),
    #[error("persisted worktree root is invalid: {path:?}")]
    InvalidWorktreeRoot { path: PathBuf },
    #[error("failed to load persisted user configuration")]
    UserConfig(#[source] BackendError),
    #[error("failed to initialize plugin lifecycle")]
    PluginLifecycle(#[source] ora_plugin_lifecycle::PluginLifecycleError),
    #[error("failed to initialize plugin management")]
    Plugin(#[source] BackendError),
    #[error("failed to synchronize installed plugin Skills")]
    PluginSkillCatalog(#[source] BackendError),
    #[error("failed to reconcile skill storage")]
    SkillStorage(#[source] ApplicationError),
    #[error("failed to initialize agent runtime")]
    AgentRuntime(#[source] BackendError),
    #[error("failed to reconcile skill storage")]
    SkillStorageReconciliation(
        #[source] crate::skill_reconciliation::SkillStorageReconciliationError,
    ),
}

/// Owns the concrete persisted use-case composition used by the Desktop adapter.
#[derive(Clone)]
pub struct Backend {
    pool: RepositoryPool,
    project: Arc<ProjectApi>,
    task: Arc<TaskApi>,
    workspace: WorkspaceApi,
    settings: Arc<Settings>,
    session: Arc<SessionApi>,
    agent_runtime: Arc<AgentRuntimeManager>,
    plugin: Arc<PluginApi>,
    skill: Arc<SkillApi>,
    agent: Arc<AgentApi>,
    workflow: Arc<WorkflowApi>,
    workflow_run: Arc<WorkflowRunApi>,
    workflow_run_engine: Arc<ConcreteWorkflowRunControl>,
    /// Serializes scheduling-affecting workflow-run mutations per run across the control entry
    /// points, the manual completion path, and the session-driver callback.
    run_locks: Arc<KeyedResourceLocks>,
    /// Transient set of node runs a manual completion is currently claiming; blocks a concurrent
    /// prompt against the same node without adding any persisted status.
    completing_node_runs: Arc<crate::workflow::run::interactive::CompletingNodeRuns>,
    sessions_root: PathBuf,
    baselines_root: PathBuf,
    app_events: Arc<AppEventHub>,
}

impl Backend {
    /// Opens persistent storage and constructs every shared CRUD API.
    ///
    /// Installed agent plugins join the built-in CLIs as agent providers; they are discovered
    /// under `paths.home_directory`, which also owns their processes and default worktrees.
    pub fn open(paths: BackendPaths) -> Result<Self, BackendBootstrapError> {
        let database_path = paths.app_data_directory.join("ora.sqlite3");
        let skills_root = paths.app_data_directory.join("atoms").join("skills");
        let sessions_root = paths.app_data_directory.join("sessions");
        let default_worktree_root = paths.home_directory.join("worktrees");
        ensure_directory(&paths.app_data_directory)?;
        let catalog = default_migration_catalog().map_err(BackendBootstrapError::Database)?;
        let pool = DatabaseBootstrapper::system()
            .bootstrap_repository_pool(&DatabaseLocation::path(&database_path), &catalog)
            .map_err(BackendBootstrapError::Database)?;
        let settings = Arc::new(Settings::new(pool.clone()));
        let stored_worktree_root = settings
            .worktree_root()
            .map_err(BackendBootstrapError::UserConfig)?;
        let configured_worktree_root = match stored_worktree_root {
            Some(root) => {
                if !root.is_absolute() || !root.is_dir() {
                    return Err(BackendBootstrapError::InvalidWorktreeRoot { path: root });
                }
                root
            }
            None => {
                ensure_directory(&default_worktree_root)?;
                default_worktree_root
            }
        };
        crate::skill_reconciliation::reconcile_skill_storage(&pool, &skills_root, &SystemClock)
            .map_err(BackendBootstrapError::SkillStorageReconciliation)?;
        crate::skill_reconciliation::cleanup_import_temp_sessions()
            .map_err(BackendBootstrapError::SkillStorageReconciliation)?;
        let clock = SystemClock;
        let app_events = Arc::new(AppEventHub::new());
        let plugin = Arc::new(
            PluginApi::open(
                pool.clone(),
                paths.home_directory.clone(),
                paths.deno_path,
                clock,
                app_events.publisher(),
                settings.clone(),
            )
            .map_err(BackendBootstrapError::Plugin)?,
        );
        plugin
            .sync_installed_skills()
            .map_err(BackendBootstrapError::PluginSkillCatalog)?;
        let scheduler = Scheduler::new(paths.timezone);
        let worktree_root = Arc::new(RwLock::new(configured_worktree_root));
        // Side files holding the worktree baseline an interactive node diffs at completion.
        let baselines_root = sessions_root.join("node-baselines");
        let relative_path_base = paths.relative_path_base;
        let agent_runtime = Arc::new(
            AgentRuntimeManager::new(AgentRuntimeSetup {
                plugin_host: plugin.clone(),
                pool: pool.clone(),
                home_directory: paths.home_directory,
                relative_path_base: relative_path_base.clone(),
                sessions_root: sessions_root.clone(),
                clock,
                scheduler,
                app_events: app_events.publisher(),
            })
            .map_err(BackendBootstrapError::AgentRuntime)?,
        );
        plugin.set_mcp_wakeup({
            let runtime = agent_runtime.clone();
            Arc::new(move || runtime.notify_mcp_desired_changed())
        });
        // Build the run engine before the crash sweep so recovery can resume stalled runs.
        let workflow_run_assembly = build_workflow_run_engine(
            agent_runtime.clone(),
            pool.clone(),
            baselines_root.clone(),
            clock,
        );
        let workflow_run_engine = workflow_run_assembly.control;
        let run_locks = workflow_run_assembly.run_locks;
        let workflow_engine = workflow_run_assembly.engine;

        // Crash recovery: fail orphaned node runs, then reconcile stalled Running runs left by a
        // previous process before serving new commands (best-effort; a failure must not block
        // startup).
        run_workflow_run_boot_sweep(&pool, &workflow_engine, &run_locks, clock);
        // Reclaim orphaned worktree-baseline side files left by a previous process.
        prune_orphaned_baselines(&pool, &baselines_root);

        // Durable Git cleanup: the worker's first pass replays every cleanup job
        // and expired provisioning lease a previous process left behind.
        let git_cleanup_worker =
            crate::git_cleanup::GitCleanupWorker::new(pool.clone(), worktree_root.clone(), clock);
        let repository_gates = git_cleanup_worker.repository_gates();
        let git_cleanup = git_cleanup_worker.spawn();

        // Durable Effect reconciliation: the first pass replays every surface a previous process
        // left short of its Desired generation, including the retirement cleanup an uninstall
        // started but could not finish.
        let effect_worker = crate::effect_worker::EffectWorker::new(
            pool.clone(),
            plugin.clone(),
            agent_runtime.clone(),
        );
        effect_worker.recover();
        // Creating a Workspace is not something a consumer declaration can observe, so both create
        // paths wake the worker to converge it promptly instead of at the next scan.
        let effect_reconcile = effect_worker.spawn();
        plugin.set_effect_reconcile(effect_reconcile.clone());

        Ok(Self {
            project: Arc::new(ProjectApi::new(
                pool.clone(),
                sessions_root.clone(),
                clock,
                effect_reconcile.clone(),
                git_cleanup.clone(),
            )),
            task: Arc::new(TaskApi::new(TaskSetup {
                pool: pool.clone(),
                worktree_root: worktree_root.clone(),
                relative_path_base: relative_path_base.clone(),
                sessions_root: sessions_root.clone(),
                repository_gates,
                clock,
                effect_reconcile: effect_reconcile.clone(),
                git_cleanup: git_cleanup.clone(),
            })),
            workspace: WorkspaceApi::new(
                pool.clone(),
                git_cleanup,
                relative_path_base,
                worktree_root,
                settings.clone(),
            ),
            settings,
            session: Arc::new(SessionApi::new(pool.clone())),
            agent_runtime,
            plugin,
            skill: Arc::new(SkillApi::new(
                pool.clone(),
                skills_root.clone(),
                clock,
                effect_reconcile,
            )),
            agent: Arc::new(AgentApi::new(pool.clone(), clock)),
            workflow: Arc::new(WorkflowApi::new(pool.clone(), clock)),
            workflow_run: Arc::new(WorkflowRunApi::new(pool.clone(), skills_root, clock)),
            workflow_run_engine,
            run_locks,
            completing_node_runs: Arc::new(std::sync::Mutex::new(std::collections::HashSet::new())),
            sessions_root,
            baselines_root,
            app_events,
            pool,
        })
    }

    /// Returns the plugin data-plane gateway the desktop surface layer drives.
    pub fn plugin_gateway(&self) -> Arc<PluginGateway> {
        Arc::new(PluginGateway::new(Arc::clone(&self.plugin)))
    }

    /// Returns the cached installed-plugin snapshot without rescanning the filesystem.
    pub fn list_installed_plugins(
        &self,
        request: ListInstalledPluginsRequest,
    ) -> Result<ListInstalledPluginsResponse, BackendError> {
        Ok(self.plugin.list(request))
    }

    /// Returns one typed Plugin Configuration editor snapshot.
    pub fn get_plugin_configuration(
        &self,
        request: GetPluginConfigurationRequest,
    ) -> Result<GetPluginConfigurationResponse, BackendError> {
        self.plugin.get_configuration(request)
    }

    /// Persists one revision-checked Plugin Configuration replacement.
    pub fn save_plugin_configuration(
        &self,
        request: SavePluginConfigurationRequest,
    ) -> Result<SavePluginConfigurationResponse, BackendError> {
        self.plugin.save_configuration(request)
    }

    /// Executes an explicit Reset All or damaged-data recovery operation.
    pub fn reset_plugin_configuration(
        &self,
        request: ResetPluginConfigurationRequest,
    ) -> Result<ResetPluginConfigurationResponse, BackendError> {
        self.plugin.reset_configuration(request)
    }

    /// Returns one Effect Target selected by opaque id or Workspace plus Agent identity.
    pub fn get_effect_target_status(
        &self,
        request: GetEffectTargetStatusRequest,
    ) -> Result<GetEffectTargetStatusResponse, BackendError> {
        EffectService::new(ora_db::SqliteEffectRepository::new(self.pool.clone()))
            .get_target_status(request)
            .map_err(|error| BackendError::internal("failed to load Effect Target status", error))
    }

    /// Returns the cached marketplace registry index used to populate plugin discovery.
    pub fn list_available_plugins(
        &self,
        request: ListAvailablePluginsRequest,
    ) -> Result<ListAvailablePluginsResponse, BackendError> {
        self.plugin.list_available_plugins(request)
    }

    /// Returns every configured marketplace source in precedence order.
    pub fn list_marketplace_sources(
        &self,
        request: ListMarketplaceSourcesRequest,
    ) -> Result<ListMarketplaceSourcesResponse, BackendError> {
        self.plugin.list_marketplace_sources(request)
    }

    /// Adds one marketplace source after validating and persisting it.
    pub fn add_marketplace_source(
        &self,
        request: AddMarketplaceSourceRequest,
    ) -> Result<AddMarketplaceSourceResponse, BackendError> {
        self.plugin.add_marketplace_source(request)
    }

    /// Removes one marketplace source by URL after persisting the new ordering.
    pub fn delete_marketplace_source(
        &self,
        request: DeleteMarketplaceSourceRequest,
    ) -> Result<DeleteMarketplaceSourceResponse, BackendError> {
        self.plugin.delete_marketplace_source(request)
    }

    /// Replaces the editable fields of one marketplace source after persisting them.
    pub fn update_marketplace_source(
        &self,
        request: UpdateMarketplaceSourceRequest,
    ) -> Result<UpdateMarketplaceSourceResponse, BackendError> {
        self.plugin.update_marketplace_source(request)
    }

    /// Pulls the marketplace source and rebuilds the cache used by plugin discovery.
    pub fn sync_available_plugins(
        &self,
        request: SyncAvailablePluginsRequest,
    ) -> Result<SyncAvailablePluginsResponse, BackendError> {
        self.plugin.sync_available_plugins(request)
    }

    /// Reads the README one marketplace listing publishes for its detail page.
    pub fn read_plugin_readme(
        &self,
        request: ReadPluginReadmeRequest,
    ) -> Result<ReadPluginReadmeResponse, BackendError> {
        self.plugin.read_plugin_readme(request)
    }

    /// Explicitly rescans packages and reconciles process-local runtime state.
    pub async fn scan_plugins(
        &self,
        request: ScanPluginsRequest,
    ) -> Result<ScanPluginsResponse, BackendError> {
        let response = self.plugin.scan(request).await?;
        self.agent_runtime.sync_plugin_agents();
        Ok(response)
    }

    /// Starts one installed plugin and returns its immediate starting state.
    pub async fn activate_plugin(
        &self,
        request: ActivatePluginRequest,
    ) -> Result<ActivatePluginResponse, BackendError> {
        self.plugin
            .activate(request)
            .await
            .map_err(BackendError::from)
    }

    /// Stops one plugin process while leaving the installed plugin available.
    pub async fn stop_plugin(
        &self,
        request: StopPluginRequest,
    ) -> Result<StopPluginResponse, BackendError> {
        self.plugin.stop(request).await.map_err(BackendError::from)
    }

    /// Stops and removes one plugin package plus its process-local state.
    pub async fn uninstall_plugin(
        &self,
        request: UninstallPluginRequest,
    ) -> Result<UninstallPluginResponse, BackendError> {
        let response = self.plugin.uninstall(request).await?;
        self.agent_runtime.sync_plugin_agents();
        Ok(response)
    }

    /// Installs a marketplace plugin by resolving its release manifest from the synced source and
    /// downloading, verifying, and extracting its package through the network-backed installer.
    ///
    /// The agent set is reconciled afterwards so the newly installed package supplies a reachable
    /// agent in this process rather than only after the next restart.
    pub async fn install_plugin(
        &self,
        request: InstallPluginRequest,
    ) -> Result<InstallPluginResponse, BackendError> {
        let response = self.plugin.install(request).await?;
        self.agent_runtime.sync_plugin_agents();
        Ok(response)
    }

    /// Installs a marketplace plugin while forwarding download progress to a host callback.
    pub async fn install_plugin_with_progress(
        &self,
        request: InstallPluginRequest,
        progress: ProgressCallback,
    ) -> Result<InstallPluginResponse, BackendError> {
        let response = self.plugin.install_with_progress(request, progress).await?;
        self.agent_runtime.sync_plugin_agents();
        Ok(response)
    }

    /// Updates one installed marketplace plugin to the version its source publishes and
    /// reconciles the agent set afterwards.
    ///
    /// The agent set is reconciled so a replaced agent package supplies a reachable agent in this
    /// process rather than only after the next restart.
    pub async fn update_plugin(
        &self,
        request: UpdatePluginRequest,
    ) -> Result<UpdatePluginResponse, BackendError> {
        let response = self.plugin.update(request).await?;
        self.agent_runtime.sync_plugin_agents();
        Ok(response)
    }

    /// Imports one local release archive and reconciles the agent set afterwards.
    ///
    /// The agent set is reconciled so the imported package supplies a reachable agent in this
    /// process rather than only after the next restart.
    pub async fn import_plugin(
        &self,
        request: ImportPluginRequest,
    ) -> Result<ImportPluginResponse, BackendError> {
        let response = self.plugin.import(request).await?;
        self.agent_runtime.sync_plugin_agents();
        Ok(response)
    }

    /// Starts a workflow run against its frozen snapshot graph.
    pub fn start_workflow_run(
        &self,
        request: StartWorkflowRunRequest,
    ) -> Result<StartWorkflowRunResponse, BackendError> {
        let _gate = self.run_locks.acquire_exclusive(request.run_id.clone());
        self.workflow_run_engine
            .start(request)
            .map_err(BackendError::from)
    }

    /// Cancels a running workflow run and stops its live node sessions.
    ///
    /// The engine commits the `Cancelled` transition first; then every session still bound to the
    /// run's node runs is stopped. Without this second step the agent keeps executing its prompt
    /// and the delete guard treats the lingering `Running` session as an active run.
    pub async fn cancel_workflow_run(
        &self,
        request: CancelWorkflowRunRequest,
    ) -> Result<CancelWorkflowRunResponse, BackendError> {
        let run_id = ora_domain::WorkflowRunId::new(&request.run_id);
        let engine = self.workflow_run_engine.clone();
        let run_locks = self.run_locks.clone();
        let response = spawn_repository_work(move || {
            // Serialize the `Cancelled` transition against every other mutation for the run; the
            // async session cleanup below runs outside the gate.
            let _gate = run_locks.acquire_exclusive(request.run_id.clone());
            engine.cancel(request).map_err(BackendError::from)
        })
        .await?;
        self.stop_workflow_run_sessions(&run_id).await;
        Ok(response)
    }

    /// Stops every agent session started for one run's node runs.
    ///
    /// Best-effort cleanup of an already-cancelled run: a session that cannot be stopped is logged
    /// rather than failing the cancel request, and sessions whose rows were deleted since attach
    /// surface as a warn because `stop_session` can no longer resolve them.
    async fn stop_workflow_run_sessions(&self, run_id: &ora_domain::WorkflowRunId) {
        let pool = self.pool.clone();
        let run_id_for_query = run_id.clone();
        let node_runs = match spawn_repository_work(move || {
            SqliteWorkflowRunEngineRepository::new(pool)
                .list_node_runs(&run_id_for_query)
                .map_err(|source| {
                    BackendError::from(ApplicationError::WorkflowRunRepository { source })
                })
        })
        .await
        {
            Ok(node_runs) => node_runs,
            Err(error) => {
                ora_warn!(run_id = %run_id, error = %error, "cancel: failed to list node runs for session cleanup");
                return;
            }
        };
        for node_run in node_runs {
            let Some(session_id) = node_run.session_id else {
                continue;
            };
            if let Err(error) = self
                .agent_runtime
                .stop_session(StopSessionRequest {
                    session_id: session_id.to_string(),
                })
                .await
            {
                ora_warn!(
                    run_id = %run_id,
                    session_id = %session_id,
                    error = %error,
                    "cancel: failed to stop workflow run session"
                );
            }
        }
    }

    /// Completes one awaiting interactive workflow node as a human request.
    ///
    /// The node is fenced first so no concurrent prompt can start, then its final assistant output
    /// and file diff are read from persisted state, the completion is committed through the engine
    /// under the per-run gate, and finally its session is stopped best-effort. Committing before
    /// stopping means a failed stop can no longer leave a "stopped session but still awaiting node"
    /// gap: once the node is terminal, prompt policy treats the session as read-only.
    pub async fn complete_workflow_node(
        &self,
        request: CompleteWorkflowNodeRequest,
    ) -> Result<CompleteWorkflowNodeResponse, BackendError> {
        let run_id = ora_domain::WorkflowRunId::new(&request.run_id);
        let node_id = request.node_id.clone();

        // Fence the node against concurrent prompts and completions before doing any expensive
        // work: once claimed, a prompt is rejected and the worktree stays stable until the commit.
        let claimed_node_run_id = {
            let pool = self.pool.clone();
            let run_locks = self.run_locks.clone();
            let completing = self.completing_node_runs.clone();
            let run_id = run_id.clone();
            let node_id = node_id.clone();
            spawn_repository_work(move || {
                crate::workflow::run::interactive::claim_node_for_completion(
                    &pool,
                    &run_locks,
                    &completing,
                    &run_id,
                    &node_id,
                )
            })
            .await?
        };

        // Prepare the final output and diff outside the gate; on failure release the claim so the
        // node returns to its awaitable state.
        let prepared = {
            let pool = self.pool.clone();
            let sessions_root = self.sessions_root.clone();
            let baselines_root = self.baselines_root.clone();
            let agent_runtime = self.agent_runtime.clone();
            let run_id = run_id.clone();
            let node_id = node_id.clone();
            match spawn_repository_work(move || {
                crate::workflow::run::interactive::prepare_completion(
                    &pool,
                    &sessions_root,
                    &baselines_root,
                    &agent_runtime,
                    &run_id,
                    &node_id,
                )
            })
            .await
            {
                Ok(prepared) => prepared,
                Err(error) => {
                    self.release_completion_claim(&claimed_node_run_id).await;
                    return Err(error);
                }
            }
        };

        // Commit the node completion under the gate and release the claim in the same critical
        // section, so a prompt cannot slip in between the commit and the release. Revalidate first:
        // a cancel that won during prepare must abort this completion rather than report success.
        let pool = self.pool.clone();
        let engine = self.workflow_run_engine.clone();
        let run_locks = self.run_locks.clone();
        let completing = self.completing_node_runs.clone();
        let node_run_id = prepared.node_run_id.clone();
        let output = prepared.output.clone();
        let structured_output = prepared.structured_output.clone();
        let stop_reason = prepared.stop_reason.clone();
        let file_changes = prepared.file_changes.clone();
        let response = spawn_repository_work(move || {
            let _gate = run_locks.acquire_exclusive(run_id.as_ref());
            let result = crate::workflow::run::interactive::revalidate_completion(
                &pool,
                &run_id,
                &node_run_id,
            )
            .and_then(|()| {
                engine
                    .complete_node(
                        &run_id,
                        &node_run_id,
                        output,
                        structured_output,
                        stop_reason,
                        file_changes,
                    )
                    .map_err(BackendError::from)
            });
            completing
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .remove(&node_run_id);
            result
        })
        .await?;

        // The node is terminal now, so its worktree baseline is no longer needed for a diff.
        let baseline_path = self
            .baselines_root
            .join(format!("{}.json", prepared.node_run_id.as_ref()));
        let _ = spawn_repository_work(move || {
            std::fs::remove_file(baseline_path).ok();
            Ok(())
        })
        .await;

        // Stop the session best-effort after the commit: the node is terminal now, so a failure
        // here only leaves a lingering session that prompt policy already treats as read-only.
        if let Some(session_id) = prepared.session_id.as_ref()
            && let Err(error) = self
                .agent_runtime
                .stop_session(StopSessionRequest {
                    session_id: session_id.to_string(),
                })
                .await
        {
            ora_warn!(session_id = %session_id, error = %error, "complete: failed to stop completed node session");
        }

        Ok(response)
    }

    /// Releases a completion claim after a prepare failure, returning the node to its awaitable
    /// state. Best-effort: a poisoned or contended completing set must not mask the real error.
    async fn release_completion_claim(&self, node_run_id: &ora_domain::WorkflowNodeRunId) {
        let completing = self.completing_node_runs.clone();
        let node_run_id = node_run_id.clone();
        let _ = spawn_repository_work(move || {
            completing
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .remove(&node_run_id);
            Ok(())
        })
        .await;
    }

    /// Restarts a finished workflow run.
    pub fn restart_workflow_run(
        &self,
        request: RestartWorkflowRunRequest,
    ) -> Result<RestartWorkflowRunResponse, BackendError> {
        let _gate = self.run_locks.acquire_exclusive(request.run_id.clone());
        self.workflow_run_engine
            .restart(request)
            .map_err(BackendError::from)
    }

    /// Sets the kickoff input of a pending workflow run.
    pub fn update_workflow_run_input(
        &self,
        request: UpdateWorkflowRunInputRequest,
    ) -> Result<UpdateWorkflowRunInputResponse, BackendError> {
        let _gate = self.run_locks.acquire_exclusive(request.run_id.clone());
        self.workflow_run_engine
            .update_input(request)
            .map_err(BackendError::from)
    }

    /// Returns the settings interface without exposing storage or runtime internals.
    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    /// Shares configurable-agent use cases without exposing repositories or runtime control.
    pub fn agents(&self) -> Arc<AgentApi> {
        self.agent.clone()
    }

    /// Shares Skill catalog/import use cases, including their Effect convergence obligations.
    pub fn skills(&self) -> Arc<SkillApi> {
        self.skill.clone()
    }

    /// Shares definition/draft/version use cases independently of workflow-run execution.
    pub fn workflows(&self) -> Arc<WorkflowApi> {
        self.workflow.clone()
    }

    /// Shares complete project use cases, including aggregate deletion and cleanup notification.
    pub fn projects(&self) -> Arc<ProjectApi> {
        self.project.clone()
    }

    /// Shares task use cases without exposing Git provisioning gates or deletion transactions.
    pub fn tasks(&self) -> Arc<TaskApi> {
        self.task.clone()
    }

    /// Shares workspace lookup, path configuration, and Git review with the existing use leases.
    pub fn workspaces(&self) -> WorkspaceApi {
        self.workspace.clone()
    }

    // =============================================================================
    // session
    // =============================================================================

    /// Creates and persists a provider session on first use.
    pub async fn start_session(
        &self,
        request: StartSessionRequest,
    ) -> Result<StartSessionResponse, BackendError> {
        self.agent_runtime.start_session(request).await
    }

    /// Applies one configuration option to a persisted session.
    pub async fn set_session_config(
        &self,
        request: SetSessionConfigRequest,
    ) -> Result<SetSessionConfigResponse, BackendError> {
        self.agent_runtime.set_session_config(request).await
    }

    /// Gets one session through the shared application composition.
    pub fn get_session(
        &self,
        request: GetSessionRequest,
    ) -> Result<GetSessionResponse, BackendError> {
        self.session.get(request).map_err(BackendError::from)
    }
    /// Lists sessions through the shared application composition.
    pub fn list_sessions(
        &self,
        request: ListSessionsRequest,
    ) -> Result<ListSessionsResponse, BackendError> {
        // Snapshot unpublished ownership before reading SQLite. If a node binding commits between
        // these reads, this snapshot still excludes the row returned by the earlier database view;
        // a later request instead sees the committed binding through the repository filter.
        let unpublished = self.agent_runtime.unpublished_workflow_session_ids()?;
        let mut response = self.session.list(request).map_err(BackendError::from)?;
        response
            .sessions
            .retain(|session| !unpublished.contains(&session.id));
        Ok(response)
    }
    /// Renames one session, locks agent title acquisition, then notifies subscribers.
    pub async fn rename_session(
        &self,
        request: RenameSessionRequest,
    ) -> Result<RenameSessionResponse, BackendError> {
        let session_id = request.session_id.clone();
        let response = self.session.rename(request).map_err(BackendError::from)?;
        if let Some(title) = response.session.title.as_deref()
            && let Ok(parsed) = ora_domain::SessionTitle::parse(title)
        {
            // A missing or busy actor must not fail the rename: the row is already updated.
            let _ = self
                .agent_runtime
                .adopt_user_title(&session_id, parsed)
                .await;
        }
        self.app_events
            .publisher()
            .try_publish(AppEvent::SessionTitleUpdated { session_id });
        Ok(response)
    }
    /// Loads one session conversation and continues its active turn when present.
    pub async fn load_session(
        &self,
        request: LoadSessionRequest,
    ) -> Result<SessionEventStream<LoadSessionEvent>, BackendError> {
        self.agent_runtime.load_session(request).await
    }

    /// Opens one subscriber to the shared application event stream.
    pub fn watch_app_events(&self) -> SessionEventStream<AppEvent> {
        self.app_events.subscribe()
    }

    /// Streams one structured ACP prompt turn for a running session.
    ///
    /// When the session belongs to an awaiting interactive workflow node, the node flips to
    /// `Running` for the duration of the turn and back to `Pending` when the turn ends or the
    /// stream is dropped, so the node's awaiting status tracks the agent's generating state.
    pub async fn prompt_session(
        &self,
        request: PromptSessionRequest,
    ) -> Result<SessionEventStream<PromptSessionEvent>, BackendError> {
        let node_run_id = crate::workflow::run::interactive::begin_human_turn(
            &self.pool,
            &self.run_locks,
            &self.completing_node_runs,
            &request.session_id,
        )
        .await?;
        let stream = match self.agent_runtime.prompt_session(request).await {
            Ok(stream) => stream,
            Err(error) => {
                // The turn never started; put the awaiting node back where it was.
                if let Some(node_run_id) = node_run_id.as_ref() {
                    let _ =
                        crate::workflow::run::interactive::end_human_turn(&self.pool, node_run_id)
                            .await;
                }
                return Err(error);
            }
        };
        let Some(node_run_id) = node_run_id else {
            return Ok(stream);
        };
        let pool = self.pool.clone();
        Ok(stream.attach_cleanup(move || {
            tokio::spawn(async move {
                let _ =
                    crate::workflow::run::interactive::end_human_turn(&pool, &node_run_id).await;
            });
        }))
    }

    /// Delivers one validated permission response to the owning session actor.
    pub async fn respond_to_session_permission(
        &self,
        request: RespondToPermissionRequest,
    ) -> Result<RespondToPermissionResponse, BackendError> {
        self.agent_runtime.respond_to_permission(request).await
    }

    /// Unloads one running session while retaining its provider history and Ora record.
    pub async fn stop_session(
        &self,
        request: StopSessionRequest,
    ) -> Result<StopSessionResponse, BackendError> {
        self.agent_runtime.stop_session(request).await
    }

    /// Cancels one active prompt while keeping its session available for another turn.
    pub fn cancel_session_prompt(
        &self,
        request: CancelSessionPromptRequest,
    ) -> Result<CancelSessionPromptResponse, BackendError> {
        self.agent_runtime.cancel_session_prompt(request)
    }

    /// Moves one existing conversation onto a different agent CLI.
    pub async fn switch_session_agent(
        &self,
        request: SwitchSessionAgentRequest,
    ) -> Result<SwitchSessionAgentResponse, BackendError> {
        self.agent_runtime.switch_agent(request).await
    }

    /// Returns a session whose history writes failed to a writable state.
    pub async fn resume_session_history(
        &self,
        request: ResumeSessionHistoryRequest,
    ) -> Result<ResumeSessionHistoryResponse, BackendError> {
        self.agent_runtime.resume_history(request).await
    }

    /// Stops one session before removing its Ora-owned record and recorded history.
    pub async fn delete_session(
        &self,
        request: DeleteSessionRequest,
    ) -> Result<DeleteSessionResponse, BackendError> {
        self.agent_runtime.delete_session(&request.session_id).await
    }

    // =============================================================================
    // agentRuntime
    // =============================================================================

    /// Reports whether each application-scoped CLI runtime is ready, starting, or unavailable.
    pub fn get_agent_runtime_status(
        &self,
        _request: GetAgentRuntimeStatusRequest,
    ) -> Result<GetAgentRuntimeStatusResponse, BackendError> {
        Ok(self.agent_runtime.agent_runtime_status())
    }

    /// Lists the models one agent advertises outside any session.
    pub async fn list_agent_models(
        &self,
        request: ListAgentModelsRequest,
    ) -> Result<ListAgentModelsResponse, BackendError> {
        self.agent_runtime.agent_models(request).await
    }

    // =============================================================================
    // gitIdentity
    // =============================================================================

    /// Reads the host identity for the sidebar profile: global git config first,
    /// falling back to the authenticated GitHub CLI account when git has no name set.
    pub fn read_git_identity(
        &self,
        _request: GetGitIdentityRequest,
    ) -> Result<GitIdentityResponse, BackendError> {
        Ok(crate::identity::resolve_git_identity())
    }

    // =============================================================================
    // workflowRun
    // =============================================================================

    /// Creates one workflow run through the shared application composition.
    pub fn create_workflow_run(
        &self,
        request: CreateWorkflowRunRequest,
    ) -> Result<CreateWorkflowRunResponse, BackendError> {
        self.workflow_run
            .create(request)
            .map_err(BackendError::from)
    }
    /// Gets one workflow run through the shared application composition.
    pub fn get_workflow_run(
        &self,
        request: GetWorkflowRunRequest,
    ) -> Result<GetWorkflowRunResponse, BackendError> {
        self.workflow_run.get(request).map_err(BackendError::from)
    }
    /// Lists workflow runs for one project through the shared application composition.
    pub fn list_workflow_runs(
        &self,
        request: ListWorkflowRunsRequest,
    ) -> Result<ListWorkflowRunsResponse, BackendError> {
        self.workflow_run.list(request).map_err(BackendError::from)
    }
    /// Lists workflow runs for one workflow through the shared application composition.
    pub fn list_workflow_runs_by_workflow(
        &self,
        request: ListWorkflowRunsByWorkflowRequest,
    ) -> Result<ListWorkflowRunsByWorkflowResponse, BackendError> {
        self.workflow_run
            .list_by_workflow(request)
            .map_err(BackendError::from)
    }
    /// Lists the node-run history of one run through the shared application composition.
    pub fn list_workflow_node_runs(
        &self,
        request: ListWorkflowNodeRunsRequest,
    ) -> Result<ListWorkflowNodeRunsResponse, BackendError> {
        self.workflow_run
            .list_node_runs(request)
            .map_err(BackendError::from)
    }
    /// Deletes one workflow run through the shared application composition.
    pub fn delete_workflow_run(
        &self,
        request: DeleteWorkflowRunRequest,
    ) -> Result<DeleteWorkflowRunResponse, BackendError> {
        self.workflow_run
            .delete(request)
            .map_err(BackendError::from)
    }

    /// Renames one workflow run through its Workspace-owned display field.
    pub fn rename_workflow_run(
        &self,
        request: RenameWorkflowRunRequest,
    ) -> Result<RenameWorkflowRunResponse, BackendError> {
        self.workflow_run
            .rename(request)
            .map_err(BackendError::from)
    }
}

/// Creates one required runtime directory and preserves its exact failing path.
fn ensure_directory(path: &Path) -> Result<(), BackendBootstrapError> {
    fs::create_dir_all(path).map_err(|source| BackendBootstrapError::DirectoryCreate {
        path: path.to_path_buf(),
        source,
    })
}

/// Fails runs interrupted by a previous process, then reconciles the survivors.
///
/// Runs that were `Running` or `Failed` when the process died have their non-terminal node runs
/// marked `Failed` with `interrupted_by_restart`. Surviving `Running` runs are then reconciled:
/// stalled ones resume scheduling, and invalid `Pending` nodes fail closed. The sweep is
/// idempotent and best-effort so a storage failure cannot block startup.
fn run_workflow_run_boot_sweep(
    pool: &RepositoryPool,
    engine: &Arc<ConcreteWorkflowRunEngine>,
    run_locks: &Arc<KeyedResourceLocks>,
    clock: SystemClock,
) {
    let repository = SqliteWorkflowRunEngineRepository::new(pool.clone());
    let run_ids = match repository.list_recoverable_runs() {
        Ok(run_ids) => run_ids,
        Err(error) => {
            ora_error!(error = %error, "workflow run boot sweep failed to list recoverable runs");
            return;
        }
    };
    if !run_ids.is_empty()
        && let Err(error) =
            repository.fail_orphaned_node_runs(&run_ids, clock.now_timestamp_millis())
    {
        ora_error!(error = %error, "workflow run boot sweep failed to fail orphaned node runs");
    }
    crate::workflow::run::reconcile_running_workflow_runs(engine, run_locks, pool);
}

/// Deletes worktree-baseline side files whose node run is missing or no longer awaiting input.
///
/// Baselines exist only while an interactive node awaits input; a crash between a node's terminal
/// commit and its baseline deletion, or a node that failed without cleanup, leaves orphaned side
/// files that this sweep reclaims at the next boot.
fn prune_orphaned_baselines(pool: &RepositoryPool, baselines_root: &Path) {
    let Ok(entries) = std::fs::read_dir(baselines_root) else {
        return;
    };
    let repository = SqliteWorkflowRunEngineRepository::new(pool.clone());
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("json") {
            continue;
        }
        let Some(name) = path.file_stem().and_then(|stem| stem.to_str()) else {
            continue;
        };
        let node_run_id = ora_domain::WorkflowNodeRunId::new(name);
        let still_awaiting = repository
            .find_node_run_by_id(&node_run_id)
            .map(|node_run| {
                node_run.is_some_and(|node| node.status == ora_domain::WorkflowNodeStatus::Pending)
            })
            .unwrap_or(false);
        if !still_awaiting {
            let _ = std::fs::remove_file(&path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Backend;
    use crate::error::ErrorClassification;
    use crate::test_backend::backend_paths;
    use ora_contracts::{
        CreateAgentRequest, CreateProjectRequest, CreateSkillRequest, DeleteAgentRequest,
        DeleteProjectRequest, DeleteSkillRequest, GetProjectRequest, ListAgentsRequest,
        ListProjectsRequest, ListSkillsRequest, UpdateAgentRequest, UpdateProjectRequest,
        UpdateSkillRequest,
    };
    use std::fs;
    use tempfile::TempDir;

    /// Verifies the shared composition owns storage bootstrap and complete non-Git CRUD flows.
    #[tokio::test]
    async fn opens_storage_and_serves_shared_crud_apis() {
        let temporary = TempDir::new().expect("create temporary backend directory");
        let database_path = temporary.path().join("data").join("ora.sqlite3");
        let worktree_root = temporary.path().join("worktrees");
        let backend = Backend::open(backend_paths(
            database_path.parent().expect("database has parent"),
            temporary.path(),
        ))
        .expect("open shared backend");

        assert!(database_path.is_file());
        assert!(worktree_root.is_dir());

        let project = backend
            .projects()
            .create(CreateProjectRequest {
                name: "Ora".to_string(),
                main_workspace_path: temporary
                    .path()
                    .join("repository")
                    .to_string_lossy()
                    .into_owned(),
            })
            .expect("create project")
            .project;
        let updated_project = backend
            .projects()
            .update(UpdateProjectRequest {
                project_id: project.id.clone(),
                name: "Ora Desktop".to_string(),
            })
            .expect("update project")
            .project;
        assert_eq!(updated_project.name, "Ora Desktop");
        assert_eq!(
            backend
                .projects()
                .list(ListProjectsRequest {})
                .expect("list projects")
                .projects,
            vec![updated_project.clone()]
        );

        let skill = backend
            .skills()
            .create(CreateSkillRequest {
                name: "review".to_string(),
                description: "Review changes".to_string(),
                content: None,
            })
            .expect("create skill")
            .skill;
        let skill = backend
            .skills()
            .update(UpdateSkillRequest {
                skill_id: skill.id,
                name: "review-code".to_string(),
                description: "Review implementation changes".to_string(),
                content: None,
            })
            .expect("update skill")
            .skill;
        assert_eq!(
            backend
                .skills()
                .list(ListSkillsRequest {})
                .expect("list skills")
                .skills,
            vec![skill.clone()]
        );

        let agent = backend
            .agents()
            .create(CreateAgentRequest {
                name: "codex".to_string(),
                description: "Coding agent".to_string(),
                content: None,
            })
            .expect("create agent")
            .agent;
        let agent = backend
            .agents()
            .update(UpdateAgentRequest {
                agent_id: agent.id,
                name: "codex-desktop".to_string(),
                description: "Desktop coding agent".to_string(),
                content: None,
            })
            .expect("update agent")
            .agent;
        assert_eq!(
            backend
                .agents()
                .list(ListAgentsRequest {})
                .expect("list agents")
                .agents,
            vec![agent.clone()]
        );

        backend
            .agents()
            .delete(DeleteAgentRequest { agent_id: agent.id })
            .expect("delete agent");
        backend
            .skills()
            .delete(DeleteSkillRequest { skill_id: skill.id })
            .expect("delete skill");
        backend
            .projects()
            .delete(DeleteProjectRequest {
                project_id: project.id.clone(),
            })
            .await
            .expect("delete project");

        let error = backend
            .projects()
            .get(GetProjectRequest {
                project_id: project.id,
            })
            .expect_err("deleted project should be hidden");
        assert_eq!(error.classification(), ErrorClassification::NotFound);
        assert_eq!(error.public_error().code(), "project_not_found");
    }

    /// Verifies startup projects installed Skill plugins into the shared Skill catalog.
    #[test]
    fn opens_with_plugin_skills_written_to_the_existing_database_schema() {
        let temporary = TempDir::new().expect("create temporary backend directory");
        let app_data_directory = temporary.path().join("app-data");
        let home_directory = temporary.path().join("ora-home");
        let package_root = home_directory.join("plugins/installed/official/review-pack/1.0.0");
        let skill_root = package_root.join("assets/review");
        fs::create_dir_all(&skill_root).expect("create installed Skill tree");
        fs::write(
            package_root.join("orax.toml"),
            "identifier = \"review-pack\"\nnamespace = \"official\"\nkind = \"skill\"\nversion = \"1.0.0\"\ndescription = \"Review skills\"\n",
        )
        .expect("write plugin manifest");
        fs::write(
            skill_root.join("SKILL.md"),
            "---\nname: review\ndescription: Reviews changes\n---\n# Review instructions\n",
        )
        .expect("write Skill manifest");

        let backend = Backend::open(backend_paths(&app_data_directory, &home_directory))
            .expect("open shared backend");

        assert!(app_data_directory.join("ora.sqlite3").is_file());
        assert!(home_directory.join("worktrees").is_dir());

        let skills = backend
            .skills()
            .list(ListSkillsRequest {})
            .expect("list plugin Skills")
            .skills;
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].namespace, "official/review-pack");
        assert_eq!(skills[0].name, "review");
        assert_eq!(
            skills[0].source,
            ora_contracts::SkillSource::Plugin {
                plugin_id: "official/review-pack".to_string(),
            }
        );
        assert_eq!(
            skills[0].availability,
            ora_contracts::SkillAvailability::Available
        );
    }
    /// Verifies an update rewrites only the manifest and preserves other package files.
    #[test]
    fn update_preserves_other_package_files() {
        let temporary = TempDir::new().expect("create temporary backend directory");
        let app_data_directory = temporary.path().join("app-data");
        let home_directory = temporary.path().join("ora-home");
        let skills_root = app_data_directory.join("atoms").join("skills");
        let backend = Backend::open(backend_paths(&app_data_directory, &home_directory))
            .expect("open shared backend");

        let skill = backend
            .skills()
            .create(CreateSkillRequest {
                name: "review".to_string(),
                description: "Reviews changes".to_string(),
                content: None,
            })
            .expect("create skill")
            .skill;
        // A user-added package file must survive an ordinary update.
        fs::create_dir_all(skills_root.join("review")).expect("create package directory");
        fs::write(skills_root.join("review").join("helper.sh"), "echo hi")
            .expect("write helper file");

        let updated = backend
            .skills()
            .update(UpdateSkillRequest {
                skill_id: skill.id,
                name: "review".to_string(),
                description: "Reviews pull requests".to_string(),
                content: None,
            })
            .expect("update skill")
            .skill;
        assert_eq!(updated.description, "Reviews pull requests");
        assert!(skills_root.join("review").join("helper.sh").is_file());
        let manifest =
            fs::read_to_string(skills_root.join("review").join("SKILL.md")).expect("read manifest");
        assert!(manifest.contains("description: Reviews pull requests"));
        assert!(!home_directory.join("atoms").exists());
    }

    /// Verifies a local Tavily MCP `.orax` import, configuration editor snapshot, and `store.json`
    /// persistence for the `apiKey` setting.
    #[tokio::test]
    async fn tavily_mcp_local_import_and_configuration() {
        use ora_contracts::{
            GetPluginConfigurationRequest, ImportPluginRequest, InstalledPluginContribution,
            ListInstalledPluginsRequest, PluginConfigurationCompleteness,
            PluginConfigurationSummary, PluginSettingValue, SavePluginConfigurationRequest,
        };
        use pretty_assertions::assert_eq;
        use std::collections::BTreeMap;
        use std::fs;
        use std::path::PathBuf;

        const PLUGIN_ID: &str = "official/ora-space.tavily-search";
        const TEST_API_KEY: &str = "tvly-test-e2e-key";

        let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../");
        let orax_archive = workspace_root.join(".tmp/ora-space.tavily-search-v0.1.0.orax");
        if !orax_archive.is_file() {
            eprintln!(
                "skipping Tavily MCP import E2E: missing {}",
                orax_archive.display()
            );
            return;
        }

        let temporary = TempDir::new().expect("create temporary backend directory");
        let data_directory = temporary.path().to_path_buf();
        let backend = Backend::open(backend_paths(&data_directory, &data_directory))
            .expect("open shared backend");

        backend
            .import_plugin(ImportPluginRequest {
                path: orax_archive.to_string_lossy().into_owned(),
            })
            .await
            .expect("import Tavily MCP release");

        let installed = backend
            .list_installed_plugins(ListInstalledPluginsRequest {})
            .expect("list installed plugins")
            .plugins
            .into_iter()
            .find(|plugin| plugin.id == PLUGIN_ID)
            .expect("Tavily MCP plugin is installed");
        assert_eq!(
            installed.contribution,
            InstalledPluginContribution::Mcp,
            "installed plugin contribution"
        );
        assert_eq!(
            installed.configuration,
            PluginConfigurationSummary::Available {
                completeness: PluginConfigurationCompleteness::Incomplete,
            },
            "configuration is incomplete before apiKey is saved"
        );

        let configuration = backend
            .get_plugin_configuration(GetPluginConfigurationRequest {
                plugin_id: PLUGIN_ID.to_string(),
            })
            .expect("load plugin configuration editor")
            .configuration;
        let api_key_setting = configuration
            .settings
            .iter()
            .find(|setting| setting.declaration.id == "apiKey")
            .expect("apiKey setting is declared");
        assert_eq!(api_key_setting.declaration.title, "API key");

        let saved = backend
            .save_plugin_configuration(SavePluginConfigurationRequest {
                preserve_setting_ids: Vec::new(),
                plugin_id: PLUGIN_ID.to_string(),
                expected_revision: configuration.revision,
                declaration_fingerprint: configuration.declaration_fingerprint.clone(),
                values: BTreeMap::from([(
                    "apiKey".to_string(),
                    PluginSettingValue::String(TEST_API_KEY.to_string()),
                )]),
            })
            .expect("save Tavily apiKey setting")
            .configuration;
        assert_eq!(
            saved.summary,
            PluginConfigurationSummary::Available {
                completeness: PluginConfigurationCompleteness::Complete,
            }
        );

        let store_json = fs::read_to_string(
            data_directory.join("plugins/data/official/ora-space.tavily-search/store.json"),
        )
        .expect("read persisted store.json");
        assert!(
            store_json.contains(TEST_API_KEY),
            "store.json should contain the saved apiKey value"
        );
    }

    /// Verifies marketplace registry resolution for the Tavily MCP listing without downloading
    /// the release archive.
    #[test]
    fn tavily_mcp_marketplace_manifest_resolves_from_staged_registry() {
        use gitlancer::BranchName;
        use ora_domain::{PluginId, PluginNamespace};
        use ora_plugin_registry::{RegistryIndex, RegistrySource};
        use pretty_assertions::assert_eq;
        use std::fs;
        use std::path::PathBuf;

        let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../");
        let marketplace_registry = workspace_root.join(".tmp/marketplace/registry");
        if !marketplace_registry.is_dir() {
            eprintln!(
                "skipping Tavily MCP marketplace manifest E2E: missing {}",
                marketplace_registry.display()
            );
            return;
        }

        let temporary = TempDir::new().expect("create temporary backend directory");
        let marketplace_checkout = temporary
            .path()
            .join("plugins/sources/github.com/ora-space/marketplace");
        fs::create_dir_all(&marketplace_checkout).expect("create marketplace checkout");
        copy_dir_recursive(
            &marketplace_registry,
            &marketplace_checkout.join("registry"),
        )
        .expect("stage marketplace registry");

        let source = RegistrySource::new(
            "https://github.com/ora-space/marketplace",
            PluginNamespace::official(),
            BranchName::new("main"),
            &marketplace_checkout,
        );
        let plugin_id = PluginId::parse("official/ora-space.tavily-search").expect("plugin id");
        let manifest = RegistryIndex::resolve_manifest_all(&[&source], &plugin_id)
            .expect("resolve marketplace manifest")
            .expect("Tavily listing is present in staged registry");
        assert_eq!(
            manifest.url().map(|locator| locator.as_str().to_string()),
            Some(
                "https://github.com/ora-space/tavily-search-mcp/releases/download/v0.1.0/ora-space.tavily-search-v0.1.0.orax"
                    .to_string()
            )
        );
        assert_eq!(
            manifest.sha256().map(|digest| digest.to_string()),
            Some("a8b58b0fc0a7c85fe774620682703149b4b6acbaa99303f399309558da282130".to_string())
        );
    }

    /// Downloads and installs Tavily from a staged marketplace registry. Requires the release URL
    /// to be reachable without authentication.
    #[tokio::test]
    #[ignore = "requires ora-space/tavily-search-mcp release assets to be publicly downloadable"]
    async fn tavily_mcp_marketplace_install_and_configuration() {
        use ora_contracts::{
            GetPluginConfigurationRequest, InstallPluginRequest, ListInstalledPluginsRequest,
            PluginConfigurationCompleteness, PluginConfigurationSummary, PluginSettingValue,
            SavePluginConfigurationRequest,
        };
        use pretty_assertions::assert_eq;
        use std::collections::BTreeMap;
        use std::fs;
        use std::path::PathBuf;

        const PLUGIN_ID: &str = "official/ora-space.tavily-search";
        const TEST_API_KEY: &str = "tvly-test-marketplace-e2e";

        let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../");
        let marketplace_registry = workspace_root.join(".tmp/marketplace/registry");
        if !marketplace_registry.is_dir() {
            eprintln!(
                "skipping Tavily MCP marketplace E2E: missing {}",
                marketplace_registry.display()
            );
            return;
        }

        let temporary = TempDir::new().expect("create temporary backend directory");
        let data_directory = temporary.path().to_path_buf();
        let marketplace_checkout =
            data_directory.join("plugins/sources/github.com/ora-space/marketplace");
        fs::create_dir_all(&marketplace_checkout).expect("create marketplace checkout");
        copy_dir_recursive(
            &marketplace_registry,
            &marketplace_checkout.join("registry"),
        )
        .expect("stage marketplace registry");

        let backend = Backend::open(backend_paths(&data_directory, &data_directory))
            .expect("open shared backend");

        backend
            .install_plugin(InstallPluginRequest {
                plugin_id: PLUGIN_ID.to_string(),
            })
            .await
            .unwrap_or_else(|error| {
                panic!(
                    "install Tavily MCP from staged marketplace registry: {error:?}. \
                     If the release URL returns 404, ensure ora-space/tavily-search-mcp is public."
                );
            });

        let installed = backend
            .list_installed_plugins(ListInstalledPluginsRequest {})
            .expect("list installed plugins")
            .plugins
            .into_iter()
            .find(|plugin| plugin.id == PLUGIN_ID)
            .expect("Tavily MCP plugin is installed after marketplace install");
        assert_eq!(
            installed.configuration,
            PluginConfigurationSummary::Available {
                completeness: PluginConfigurationCompleteness::Incomplete,
            }
        );

        let configuration = backend
            .get_plugin_configuration(GetPluginConfigurationRequest {
                plugin_id: PLUGIN_ID.to_string(),
            })
            .expect("load plugin configuration editor")
            .configuration;
        backend
            .save_plugin_configuration(SavePluginConfigurationRequest {
                preserve_setting_ids: Vec::new(),
                plugin_id: PLUGIN_ID.to_string(),
                expected_revision: configuration.revision,
                declaration_fingerprint: configuration.declaration_fingerprint.clone(),
                values: BTreeMap::from([(
                    "apiKey".to_string(),
                    PluginSettingValue::String(TEST_API_KEY.to_string()),
                )]),
            })
            .expect("save Tavily apiKey after marketplace install");

        let store_json = fs::read_to_string(
            data_directory.join("plugins/data/official/ora-space.tavily-search/store.json"),
        )
        .expect("read persisted store.json");
        assert!(
            store_json.contains(TEST_API_KEY),
            "store.json should contain the saved apiKey value"
        );
    }

    /// When `ORA_E2E_PLUGIN_DATA` points at a live Desktop plugin home, verifies Tavily settings
    /// persistence against the real on-disk layout.
    #[tokio::test]
    async fn tavily_mcp_save_configuration_in_desktop_plugin_home() {
        use ora_contracts::{
            GetPluginConfigurationRequest, ListInstalledPluginsRequest,
            PluginConfigurationCompleteness, PluginConfigurationSummary, PluginSettingValue,
            SavePluginConfigurationRequest,
        };
        use std::collections::BTreeMap;
        use std::fs;
        use std::path::PathBuf;

        const PLUGIN_ID: &str = "official/ora-space.tavily-search";
        const TEST_API_KEY: &str = "tvly-desktop-e2e-key";

        let Ok(data_directory) = std::env::var("ORA_E2E_PLUGIN_DATA") else {
            return;
        };
        let data_directory = PathBuf::from(data_directory);
        let plugin_data_directory = data_directory.clone();
        if !data_directory.is_dir() {
            eprintln!(
                "skipping desktop-home Tavily E2E: {} is not a directory",
                data_directory.display()
            );
            return;
        }

        let temporary = TempDir::new().expect("create temporary backend directory");
        let backend = Backend::open(backend_paths(temporary.path(), &data_directory))
            .expect("open shared backend");

        let installed = backend
            .list_installed_plugins(ListInstalledPluginsRequest {})
            .expect("list installed plugins")
            .plugins
            .into_iter()
            .find(|plugin| plugin.id == PLUGIN_ID)
            .expect("Tavily MCP plugin is installed in desktop plugin home");
        assert!(
            matches!(
                installed.configuration,
                PluginConfigurationSummary::Available {
                    completeness: PluginConfigurationCompleteness::Incomplete,
                } | PluginConfigurationSummary::Available {
                    completeness: PluginConfigurationCompleteness::Complete,
                }
            ),
            "configuration should be available after the dotted-name store-path fix"
        );

        let configuration = backend
            .get_plugin_configuration(GetPluginConfigurationRequest {
                plugin_id: PLUGIN_ID.to_string(),
            })
            .expect("load plugin configuration editor")
            .configuration;
        if matches!(
            configuration.summary,
            PluginConfigurationSummary::Available {
                completeness: PluginConfigurationCompleteness::Complete,
            }
        ) {
            return;
        }
        backend
            .save_plugin_configuration(SavePluginConfigurationRequest {
                preserve_setting_ids: Vec::new(),
                plugin_id: PLUGIN_ID.to_string(),
                expected_revision: configuration.revision,
                declaration_fingerprint: configuration.declaration_fingerprint.clone(),
                values: BTreeMap::from([(
                    "apiKey".to_string(),
                    PluginSettingValue::String(TEST_API_KEY.to_string()),
                )]),
            })
            .expect("save Tavily apiKey in desktop plugin home");

        let store_json = fs::read_to_string(
            plugin_data_directory.join("plugins/data/official/ora-space.tavily-search/store.json"),
        )
        .expect("read persisted store.json");
        assert!(
            store_json.contains(TEST_API_KEY),
            "store.json should contain the saved apiKey value"
        );
    }

    /// Recursively copies one directory tree for marketplace registry staging in tests.
    fn copy_dir_recursive(from: &std::path::Path, to: &std::path::Path) -> std::io::Result<()> {
        fs::create_dir_all(to)?;
        for entry in fs::read_dir(from)? {
            let entry = entry?;
            let destination = to.join(entry.file_name());
            if entry.file_type()?.is_dir() {
                copy_dir_recursive(&entry.path(), &destination)?;
            } else {
                fs::copy(entry.path(), destination)?;
            }
        }
        Ok(())
    }
}
