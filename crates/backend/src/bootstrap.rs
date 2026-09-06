use crate::agent::AgentApi;
use crate::agent_runtime::{AgentRuntimeManager, AgentRuntimeSetup, SessionEventStream};
use crate::app_event::AppEventHub;
use crate::clock::SystemClock;
use crate::error::BackendError;
use crate::git_cleanup::KeyedResourceLocks;
use crate::plugin::{PluginApi, Plugins};
use crate::project::ProjectApi;
use crate::session::SessionApi;
use crate::settings::Settings;
use crate::skill::SkillApi;
use crate::task::{TaskApi, TaskSetup};
use crate::workflow::WorkflowApi;
use crate::workflow::run::{WorkflowRunSetup, WorkflowRuns};
use crate::workflow::run::{
    build_workflow_run_engine, prune_orphaned_baselines, run_workflow_run_boot_sweep,
};
use crate::workspace::WorkspaceApi;
use ora_application::{ApplicationError, EffectService};
use ora_contracts::*;
use ora_db::{DatabaseBootstrapper, DatabaseLocation, RepositoryPool, default_migration_catalog};
use ora_scheduler::Scheduler;
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
    plugin: Plugins,
    skill: Arc<SkillApi>,
    agent: Arc<AgentApi>,
    workflow: Arc<WorkflowApi>,
    workflow_run: Arc<WorkflowRuns>,
    /// Serializes scheduling-affecting workflow-run mutations per run across the control entry
    /// points, the manual completion path, and the session-driver callback.
    run_locks: Arc<KeyedResourceLocks>,
    /// Transient set of node runs a manual completion is currently claiming; blocks a concurrent
    /// prompt against the same node without adding any persisted status.
    completing_node_runs: Arc<crate::workflow::run::interactive::CompletingNodeRuns>,
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

        let completing_node_runs =
            Arc::new(std::sync::Mutex::new(std::collections::HashSet::new()));
        let workflow_run = Arc::new(WorkflowRuns::new(WorkflowRunSetup {
            pool: pool.clone(),
            skills_root: skills_root.clone(),
            sessions_root: sessions_root.clone(),
            baselines_root,
            agent_runtime: agent_runtime.clone(),
            engine: workflow_run_engine,
            run_locks: run_locks.clone(),
            completing_node_runs: completing_node_runs.clone(),
            clock,
        }));

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
                sessions_root,
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
            plugin: Plugins::new(plugin, agent_runtime.clone()),
            agent_runtime,
            skill: Arc::new(SkillApi::new(
                pool.clone(),
                skills_root,
                clock,
                effect_reconcile,
            )),
            agent: Arc::new(AgentApi::new(pool.clone(), clock)),
            workflow: Arc::new(WorkflowApi::new(pool.clone(), clock)),
            workflow_run,
            run_locks,
            completing_node_runs,
            app_events,
            pool,
        })
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

    /// Shares run use cases, including scheduling serialization and terminal session cleanup.
    pub fn workflow_runs(&self) -> Arc<WorkflowRuns> {
        self.workflow_run.clone()
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

    /// Shares plugin use cases together with the runtime reconciliation each mutation requires.
    pub fn plugins(&self) -> Plugins {
        self.plugin.clone()
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
}

/// Creates one required runtime directory and preserves its exact failing path.
fn ensure_directory(path: &Path) -> Result<(), BackendBootstrapError> {
    fs::create_dir_all(path).map_err(|source| BackendBootstrapError::DirectoryCreate {
        path: path.to_path_buf(),
        source,
    })
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
}
