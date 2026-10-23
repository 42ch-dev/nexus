//! Pre-selection home control owned by `CoreHomeService` (v1.190 P2-T0).
//!
//! A narrow home/configuration entry extracted from the CLI/home helpers and
//! the daemon workspace handlers: persistent local-creator registration
//! (global identity store + active-selection config), workspace discovery,
//! active workspace selection (which initializes only the chosen workspace's
//! guarded state DB), and resolved home configuration. Storage diagnostics
//! live in [`crate::storage_status`].
//!
//! It is a home-control entry to the same authority, not a second domain
//! engine or pool factory: no workspace pool is held across calls and no
//! execution/Host is started. Actual workspace commands still use
//! [`crate::CoreService`], which stays workspace-bound; a successful
//! identity/workspace change invalidates the old selection on the next
//! disk re-read (`CoreService::verify_selected_context`).

use std::path::{Path, PathBuf};

use nexus_contracts::generated::daemon_api::workspace::{
    create_workspace_request::CreateWorkspaceRequest,
    create_workspace_response::CreateWorkspaceResponse,
    list_workspaces_response::{NexusPaginationInfo, NexusWorkspaceSummary},
    ListWorkspacesResponse, SetActiveWorkspaceRequest, SetActiveWorkspaceResponse,
};
use nexus_contracts::CreatorDetail;
use nexus_contracts::{CoreHomeConfiguration, CoreRegisterCreatorRequest};
use nexus_home_layout::active_context::{read_active_creator_id, CliConfigSnapshot};
use nexus_home_layout::{
    operational_workspace_dir, validate_creator_id_safe, workspace_state_db_path,
};
use nexus_local_db::open_pool_read_only;

use crate::error::{local_db_err, CoreError, CoreResult};

/// Maximum length for a creator display name — parity with the CLI identity
/// front door (`MAX_CREATOR_NAME_LENGTH` in `nexus42`), so both entry points
/// reject the same over-long display token.
pub const MAX_CREATOR_NAME_LENGTH: usize = 64;

/// Serializes workspace creation in-process.
///
/// The create transaction is check-then-write (slug registered? root claimed?
/// root empty? probe writable? then materialize). Two concurrent creates of the
/// same slug — or of two slugs claiming the same override root — can both pass
/// the checks before either registers, so the whole sequence runs under this
/// lock. It must be process-global, not per instance: [`CoreHomeService`] is
/// opened per call and holds no state between calls.
static CREATE_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Opaque home-control entry point. The layout is private; callers only pass
/// the raw user home to [`CoreHomeService::open`] and await the typed
/// operations. No workspace pool is held between calls.
pub struct CoreHomeService {
    pub(crate) user_home: PathBuf,
    pub(crate) nexus_home: PathBuf,
}

impl CoreHomeService {
    /// Open the home-control entry for `user_home` (raw user home semantics:
    /// home-layout appends `.nexus42` exactly once).
    ///
    /// No selected workspace is assumed and nothing is created on disk —
    /// this succeeds on a temporary empty home.
    ///
    /// # Errors
    /// Returns [`CoreError::InvalidInput`] when `user_home` is not absolute.
    pub fn open(user_home: PathBuf) -> CoreResult<Self> {
        if !user_home.is_absolute() {
            return Err(CoreError::InvalidInput {
                field: "user_home".to_string(),
                reason: "must be an absolute path".to_string(),
            });
        }
        Ok(Self {
            nexus_home: nexus_home_layout::nexus_root_from_home(&user_home),
            user_home,
        })
    }

    /// The raw user home this entry was opened against.
    #[must_use]
    pub fn user_home(&self) -> &Path {
        &self.user_home
    }

    /// Register a persistent local creator in the global identity store
    /// (`<nexus_home>/state.db`) and select it as the active creator.
    ///
    /// Extracted from the CLI local-creator bootstrap (`creator register
    /// --local` / `system identity create --persistent`): display-name
    /// validation (trimmed, non-empty, no control characters, ≤64 bytes),
    /// the byte-exact name-collision decision tree over persistent
    /// identities, the `ctr_local<12 hex>` mint, and the active-creator
    /// config write. The workspace `creators` row and workspace state DB are
    /// NOT materialized here — a home registration cannot assume a workspace;
    /// selecting one ([`Self::select_workspace`]) initializes only the chosen
    /// workspace.
    ///
    /// When `request.display_name` is `Some`, a matching persistent identity
    /// converges to it (repair/activation, never a second mint) and 2+
    /// matches are an honest collision error. When `platform_creator_id` is
    /// `Some`, the minted identity is linked to that platform creator through
    /// the existing identity-store link.
    ///
    /// # Errors
    /// Returns [`CoreError::InvalidInput`] for invalid display names or
    /// platform ids and for display-name collisions, and the mapped storage
    /// error when the global identity store or the config write fails.
    #[allow(clippy::too_many_lines)] // one linear domain operation
    pub async fn register_creator(
        &self,
        request: CoreRegisterCreatorRequest,
    ) -> CoreResult<CreatorDetail> {
        let display_name = validated_display_name(
            request
                .display_name
                .as_deref()
                .map(std::string::String::as_str),
        )?;
        if let Some(platform_id) = request.platform_creator_id.as_deref() {
            validate_creator_id_safe(platform_id).map_err(|reason| CoreError::InvalidInput {
                field: "platform_creator_id".to_string(),
                reason,
            })?;
        }

        let global_db = self.nexus_home.join("state.db");

        // Read-only collision/convergence decision tree (no store writes when
        // the registration is a no-op or a collision).
        if let Some(trimmed) = display_name.as_deref() {
            if global_db.exists() {
                let matches = self.read_persistent_matches(&global_db, trimmed).await?;
                match matches.len() {
                    0 => {}
                    1 => {
                        return self
                            .converge_identity(
                                &matches[0].creator_id,
                                matches[0].display_name.as_deref(),
                            )
                            .await;
                    }
                    _ => return Err(collision_error(trimmed, &matches)),
                }
            }
        } else {
            let cfg = load_config_snapshot(&self.nexus_home)?;
            if let Some(active_id) = cfg.active_creator_id {
                if global_db.exists() {
                    let row = self.read_identity_row(&global_db, &active_id).await?;
                    if let Some(row) = row {
                        if row.identity_type == "persistent" {
                            return self
                                .converge_identity(&row.creator_id, row.display_name.as_deref())
                                .await;
                        }
                    }
                }
            }
        }

        // Mint leg: writable global store (migrations + seed versions via the
        // guarded initialization factory), INSERT, optional platform link,
        // then the active-creator config write.
        std::fs::create_dir_all(&self.nexus_home).map_err(|e| CoreError::Internal {
            category: format!("home_dir_create: {e}"),
        })?;
        let pool = nexus_local_db::init_pool(&global_db)
            .await
            .map_err(local_db_err)?;
        let creator_id = mint_local_creator_id();
        let created_at = chrono::Utc::now().to_rfc3339();
        let inserted = nexus_local_db::create_local_identity(
            &pool,
            &creator_id,
            "persistent",
            display_name.as_deref(),
            &created_at,
        )
        .await;
        if let Err(err) = inserted {
            // The unique partial index on persistent display_name rejects a
            // name minted concurrently between our 0-match read and this
            // INSERT (TOCTOU): surface the same honest collision. The
            // collision read must run on the still-open pool — reading after
            // the close would turn a concurrent same-name registration into
            // an internal closed-pool failure instead of the collision error.
            if let nexus_local_db::LocalDbError::Sqlx(sqlx::Error::Database(db_err)) = &err {
                if db_err.is_unique_violation() {
                    let collision = match nexus_local_db::list_local_identities(&pool).await {
                        Ok(rows) => {
                            let display = display_name.unwrap_or_else(|| creator_id.clone());
                            let matches = persistent_rows_with_name(&rows, &display);
                            if matches.len() > 1 {
                                collision_error(&display, &matches)
                            } else {
                                CoreError::InvalidInput {
                                    field: "display_name".to_string(),
                                    reason: format!(
                                        "creator name collision: {display} is already used by a persistent identity"
                                    ),
                                }
                            }
                        }
                        Err(read_err) => local_db_err(read_err),
                    };
                    pool.close().await;
                    return Err(collision);
                }
            }
            pool.close().await;
            return Err(local_db_err(err));
        }
        if let Some(platform_id) = request.platform_creator_id.as_deref() {
            if let Err(err) =
                nexus_local_db::link_to_platform(&pool, &creator_id, platform_id).await
            {
                pool.close().await;
                return Err(local_db_err(err));
            }
        }
        pool.close().await;

        write_active_selection(&self.nexus_home, &creator_id, None)?;

        Ok(CreatorDetail {
            creator_id,
            // Registration activates the global identity; the workspace
            // `creators` row (and with it the holder registry row) is
            // materialized by the workspace-selection path, so no registry
            // state is at hand to project here.
            holder_entry_id: None,
            display_name,
            handle: None,
            has_api_key: false,
            has_cached_token: false,
            is_active: true,
        })
    }

    /// Converge an existing persistent identity: make it the active creator
    /// when it is not, and return its detail. Read paths stay read-only —
    /// no identity-row or workspace-row writes (a home registration cannot
    /// assume a workspace).
    #[allow(clippy::unused_async_trait_impl)] // async matches the trait contract; the impl has no await today
    #[allow(clippy::unused_async)] // async is the await-symmetric public signature; the body is store-only today
    async fn converge_identity(
        &self,
        creator_id: &str,
        display_name: Option<&str>,
    ) -> CoreResult<CreatorDetail> {
        let cfg = load_config_snapshot(&self.nexus_home)?;
        if cfg.active_creator_id.as_deref() != Some(creator_id) {
            write_active_selection(&self.nexus_home, creator_id, None)?;
        }
        Ok(CreatorDetail {
            creator_id: creator_id.to_string(),
            // Convergence is a config/selection read: it holds no workspace
            // pool, so there is no registry state to project.
            holder_entry_id: None,
            display_name: display_name.map(str::to_string),
            handle: None,
            has_api_key: false,
            has_cached_token: false,
            is_active: true,
        })
    }

    /// List all registered workspaces (creator directories under
    /// `<nexus_home>/creators` containing an operational `meta.json`), sorted
    /// by `(creator_id, workspace_slug)`.
    ///
    /// The response covers the full discovery set: `pagination` reports the
    /// whole list (`has_more: false`, no cursor); transports apply their own
    /// retained query/filter/pagination on top.
    ///
    /// # Errors
    /// Returns the mapped storage error only; unreadable directories are
    #[allow(clippy::unused_async_trait_impl)] // async matches the trait contract; the impl has no await today
    /// skipped, mirroring the extracted daemon scan.
    #[allow(clippy::unused_async)] // async is the await-symmetric public signature; the body is store-only today
    pub async fn list_workspaces(&self) -> CoreResult<ListWorkspacesResponse> {
        let items = scan_workspaces(&self.nexus_home);
        let limit = i64::try_from(items.len()).unwrap_or(i64::MAX);
        Ok(ListWorkspacesResponse {
            items,
            pagination: NexusPaginationInfo {
                limit,
                has_more: false,
                next_cursor: None,
            },
        })
    }

    /// Create and register a workspace for the active creator (I-000063).
    ///
    /// The workspace id IS the `(creator_id, workspace_slug)` pair: the slug is
    /// client-proposed and core-validated, and `creator_id` resolves against
    /// the active creator — a mismatching request is refused rather than
    /// trusted. The creative root is either the caller's override or the one
    /// shared derivation ([`default_creative_root`]).
    ///
    /// The check → probe → materialize transaction runs under a process-global
    /// async mutex ([`CREATE_LOCK`]): two concurrent creates can otherwise both
    /// pass the preflight (slug absent, root empty) before either registers.
    /// `meta.json` (the registration discovery reads) is written last; a
    /// failure removes only the artifacts this call produced, never a
    /// pre-existing directory that could already hold a sibling registration.
    ///
    /// # Errors
    /// [`CoreError::Uninitialized`] with no active creator,
    /// [`CoreError::InvalidInput`] for an invalid slug, display name or root
    /// override (including an unwritable or unreadable target), and
    /// [`CoreError::WorkspaceConflict`] when the slug, the resolved root or the
    /// target directory content is already claimed.
    #[allow(clippy::too_many_lines)] // one linear create transaction
    pub async fn create_workspace(
        &self,
        request: CreateWorkspaceRequest,
    ) -> CoreResult<CreateWorkspaceResponse> {
        validate_workspace_slug(&request.workspace_slug)?;
        let creator_id =
            read_active_creator_id(&self.nexus_home).ok_or(CoreError::Uninitialized)?;
        if request.creator_id != creator_id {
            return Err(CoreError::InvalidInput {
                field: "creator_id".to_string(),
                reason: "must match the active creator".to_string(),
            });
        }
        let slug = request.workspace_slug;

        // Serialize the whole check+register transaction; see `CREATE_LOCK`.
        let _create_guard = CREATE_LOCK.lock().await;

        let op_dir = operational_workspace_dir(&self.user_home, &creator_id, &slug);
        let op_dir_preexisting = op_dir.exists();
        if op_dir.join("meta.json").exists() {
            return Err(CoreError::WorkspaceConflict(format!(
                "workspace '{slug}' already exists for creator {creator_id}"
            )));
        }

        let creative_root = match request.creative_root.as_deref().map(str::trim) {
            Some(raw) if !raw.is_empty() => {
                validate_creative_root_override(raw)?;
                PathBuf::from(raw)
            }
            _ => default_creative_root(&self.user_home, &self.nexus_home, &creator_id, &slug),
        };
        if scan_workspaces(&self.nexus_home).iter().any(|item| {
            !item.creative_root.is_empty() && Path::new(&item.creative_root) == creative_root
        }) {
            return Err(CoreError::WorkspaceConflict(format!(
                "creative root {} is already claimed by another workspace",
                creative_root.display()
            )));
        }

        // `Some` means the target already existed: it is adopted only when
        // empty, and a later failure removes just the artifacts we added.
        let preexisting_root = creative_root.is_dir().then_some(creative_root.as_path());
        if let Some(root) = preexisting_root {
            if !directory_is_empty(root)? {
                return Err(CoreError::WorkspaceConflict(format!(
                    "creative root {} already holds content",
                    creative_root.display()
                )));
            }
        }
        if let Err(err) = probe_creative_dir(&creative_root) {
            // The probe is the first writer; undo the root it created.
            if preexisting_root.is_none() {
                let _ = std::fs::remove_dir_all(&creative_root);
            }
            return Err(err);
        }

        let db_path = workspace_state_db_path(&self.user_home, &creator_id, &slug);
        let db_preexisting = db_path.exists();
        let manifest_path = creative_root.join(".nexus42").join("workspace.json");
        let manifest_preexisting = manifest_path.exists();
        if let Err(err) = self
            .materialize_new_workspace(
                &creator_id,
                &slug,
                &creative_root,
                &db_path,
                request.display_name.as_deref(),
            )
            .await
        {
            rollback_new_workspace(
                &op_dir,
                op_dir_preexisting,
                &db_path,
                db_preexisting,
                &manifest_path,
                manifest_preexisting,
                &creative_root,
                preexisting_root.is_some(),
            );
            return Err(err);
        }

        Ok(CreateWorkspaceResponse {
            creator_id,
            workspace_slug: slug,
            creative_root: creative_root.display().to_string(),
            operational_dir: op_dir.display().to_string(),
            state_db_path: db_path.display().to_string(),
        })
    }

    /// Write a new workspace's artifacts in failure order: operational dir,
    /// creative manifest, guarded state DB, then — last — `meta.json`.
    async fn materialize_new_workspace(
        &self,
        creator_id: &str,
        slug: &str,
        creative_root: &Path,
        db_path: &Path,
        display_name: Option<&str>,
    ) -> CoreResult<()> {
        let op_dir = operational_workspace_dir(&self.user_home, creator_id, slug);
        std::fs::create_dir_all(&op_dir).map_err(|e| CoreError::Internal {
            category: format!("workspace_dir_create: {e}"),
        })?;

        let display_name = validated_display_name(display_name)?.unwrap_or_else(|| {
            if slug == "default" {
                "默认工作区".to_string()
            } else {
                slug.to_string()
            }
        });
        let creative_meta = creative_root.join(".nexus42");
        std::fs::create_dir_all(&creative_meta).map_err(|e| CoreError::Internal {
            category: format!("workspace_manifest_dir_create: {e}"),
        })?;
        let manifest = serde_json::json!({
            "name": display_name,
            "version": 1,
            "created_at": chrono::Utc::now().to_rfc3339(),
            "creator_id": creator_id,
            "workspace_slug": slug,
        });
        write_json_file(
            &creative_meta.join("workspace.json"),
            &manifest,
            "workspace_manifest",
        )?;

        let pool = nexus_local_db::init_pool(db_path)
            .await
            .map_err(local_db_err)?;
        pool.close().await;

        let meta = serde_json::json!({
            "schema_version": 1,
            "creator_id": creator_id,
            "workspace_slug": slug,
            "local_root": creative_root.display().to_string(),
            "workspace_id": null,
            "created_at": chrono::Utc::now().to_rfc3339(),
        });
        write_json_file(&op_dir.join("meta.json"), &meta, "workspace_meta")
    }

    /// Select the active creator/workspace and initialize only the chosen
    /// workspace (guarded state-DB initialization + workspace `creators` row).
    ///
    /// Extracted from the daemon `set_active_workspace` handler: the request
    /// creator defaults to the currently active one, the workspace must exist
    /// on disk, and a registered `meta.json` naming a different creator or
    /// slug is a foreign-workspace denial. The chosen workspace is initialized
    /// first (guarded state DB + `creators` row) and the selection is
    /// committed to `config.toml` (unknown keys preserved) only after that
    /// succeeds, so a failed initialization leaves the previous selection —
    /// and the principals minted under it — intact. A successful change
    /// invalidates the previous selection for any open
    /// [`crate::CoreService`] on its next disk re-read.
    ///
    /// # Errors
    /// Returns [`CoreError::Uninitialized`] when no creator can be resolved,
    /// [`CoreError::InvalidInput`] for unsafe ids/slugs, [`CoreError::NotFound`]
    /// for missing or foreign workspaces, and the mapped storage error when
    /// the guarded initialization or config write fails.
    pub async fn select_workspace(
        &self,
        request: SetActiveWorkspaceRequest,
    ) -> CoreResult<SetActiveWorkspaceResponse> {
        let creator_id = match request.creator_id {
            Some(cid) => {
                validate_creator_id_safe(&cid).map_err(|reason| CoreError::InvalidInput {
                    field: "creator_id".to_string(),
                    reason,
                })?;
                cid
            }
            None => read_active_creator_id(&self.nexus_home).ok_or(CoreError::Uninitialized)?,
        };
        validate_workspace_slug(&request.workspace_slug)?;

        let op_dir =
            operational_workspace_dir(&self.user_home, &creator_id, &request.workspace_slug);
        if !op_dir.is_dir() {
            return Err(CoreError::NotFound {
                resource: format!(
                    "Workspace {} does not exist for creator {}",
                    request.workspace_slug, creator_id
                ),
            });
        }
        deny_foreign_workspace(&op_dir, &creator_id, &request.workspace_slug)?;

        // Initialize only the chosen workspace BEFORE committing the
        // selection: guarded initialization factory (Direct-writer admission
        // + migrations + seed versions) and the workspace `creators` row the
        // FK-prechecks rely on. The pool is closed before returning — the
        // home entry holds no workspace pool.
        let db_path =
            workspace_state_db_path(&self.user_home, &creator_id, &request.workspace_slug);
        let row_display_name = self.resolved_display_name(&creator_id).await;
        let pool = nexus_local_db::init_pool(&db_path)
            .await
            .map_err(local_db_err)?;
        let materialized =
            nexus_local_db::ensure_creator_row(&pool, &creator_id, &row_display_name).await;
        pool.close().await;
        materialized.map_err(local_db_err)?;

        // A creator bootstrap only pre-creates the directory: register the
        // workspace's discovery metadata so it is listable (D25 — the first
        // workspace is selected, never created over HTTP).
        self.ensure_workspace_registration(&creator_id, &request.workspace_slug)?;

        // Failure-atomic selection commit: the config write happens only
        // after initialization fully succeeded, so an initialization failure
        // preserves the previous selection instead of committing a broken one.
        write_active_selection(&self.nexus_home, &creator_id, Some(&request.workspace_slug))?;

        Ok(SetActiveWorkspaceResponse {
            creator_id,
            workspace_slug: request.workspace_slug,
        })
    }

    /// Register the selected workspace for discovery when only its operational
    /// directory was pre-created. `scan_workspaces` lists exactly the
    /// workspaces carrying an operational `meta.json`, and the display name is
    /// read from `<creative_root>/.nexus42/workspace.json`; both are written
    /// here (never overwriting an existing registration or manifest).
    ///
    /// # Errors
    /// Returns the mapped internal carriers when the creative tree or the
    /// registration metadata cannot be written.
    fn ensure_workspace_registration(&self, creator_id: &str, slug: &str) -> CoreResult<()> {
        let op_dir = operational_workspace_dir(&self.user_home, creator_id, slug);
        if op_dir.join("meta.json").exists() {
            return Ok(());
        }

        let creative_root =
            default_creative_root(&self.user_home, &self.nexus_home, creator_id, slug);
        let creative_meta = creative_root.join(".nexus42");
        std::fs::create_dir_all(&creative_meta).map_err(|e| CoreError::Internal {
            category: format!("workspace_dir_create: {e}"),
        })?;

        let workspace_json = creative_meta.join("workspace.json");
        if !workspace_json.exists() {
            // D25 copy: the pre-created first workspace is the product's
            // `默认工作区`; an additional workspace names itself by slug (the
            // deferred create route supplies its own display name).
            let display_name = if slug == "default" {
                "默认工作区"
            } else {
                slug
            };
            let manifest = serde_json::json!({
                "name": display_name,
                "version": 1,
                "created_at": chrono::Utc::now().to_rfc3339(),
                "creator_id": creator_id,
                "workspace_slug": slug,
            });
            let text =
                serde_json::to_string_pretty(&manifest).map_err(|e| CoreError::Internal {
                    category: format!("workspace_manifest_serialize: {e}"),
                })?;
            std::fs::write(&workspace_json, text).map_err(|e| CoreError::Internal {
                category: format!("workspace_manifest_write: {e}"),
            })?;
        }

        let meta = serde_json::json!({
            "schema_version": 1,
            "creator_id": creator_id,
            "workspace_slug": slug,
            "local_root": creative_root,
            "workspace_id": null,
            "created_at": chrono::Utc::now().to_rfc3339(),
        });
        let text = serde_json::to_string_pretty(&meta).map_err(|e| CoreError::Internal {
            category: format!("workspace_meta_serialize: {e}"),
        })?;
        std::fs::write(op_dir.join("meta.json"), text).map_err(|e| CoreError::Internal {
            category: format!("workspace_meta_write: {e}"),
        })
    }

    /// The resolved home configuration: active creator/workspace selection
    /// plus the resolved `nexus_home` and raw `user_home` roots.
    ///
    /// `None` selections describe an uninitialized shell; this is not an
    /// error at the core boundary (transports keep their retained
    /// uninitialized responses).
    ///
    /// # Errors
    /// Returns [`CoreError::Internal`] when the config file cannot be read or
    #[allow(clippy::unused_async_trait_impl)] // async matches the trait contract; the impl has no await today
    /// parsed.
    #[allow(clippy::unused_async)] // selected contract shape: async for transport symmetry
    pub async fn configuration(&self) -> CoreResult<CoreHomeConfiguration> {
        let cfg = load_config_snapshot(&self.nexus_home)?;
        let active_workspace_slug = cfg
            .active_creator_id
            .as_ref()
            .map(|creator| cfg.workspace_slug_for_creator(creator));
        Ok(CoreHomeConfiguration {
            active_creator_id: cfg.active_creator_id,
            active_workspace_slug,
            nexus_home: newtype_value(&self.nexus_home.display().to_string())?,
            user_home: newtype_value(&self.user_home.display().to_string())?,
        })
    }

    /// Persistent identity rows in the global store whose display name is
    /// byte-exactly `trimmed`, read through a read-only pool.
    async fn read_persistent_matches(
        &self,
        global_db: &Path,
        trimmed: &str,
    ) -> CoreResult<Vec<nexus_local_db::LocalIdentityRow>> {
        let pool = open_pool_read_only(global_db).await.map_err(local_db_err)?;
        let rows = nexus_local_db::list_local_identities(&pool)
            .await
            .map_err(local_db_err)?;
        pool.close().await;
        Ok(persistent_rows_with_name(&rows, trimmed))
    }

    /// One identity row from the global store, read through a read-only pool.
    async fn read_identity_row(
        &self,
        global_db: &Path,
        creator_id: &str,
    ) -> CoreResult<Option<nexus_local_db::LocalIdentityRow>> {
        let pool = open_pool_read_only(global_db).await.map_err(local_db_err)?;
        let row = nexus_local_db::get_local_identity(&pool, creator_id)
            .await
            .map_err(local_db_err)?;
        pool.close().await;
        Ok(row)
    }

    /// Display name for `creator_id` from the global identity store when it
    /// exists, falling back to the id (mirrors the CLI bootstrap projection).
    async fn resolved_display_name(&self, creator_id: &str) -> String {
        let global_db = self.nexus_home.join("state.db");
        if !global_db.exists() {
            return creator_id.to_string();
        }
        match self.read_identity_row(&global_db, creator_id).await {
            Ok(Some(row)) => row.display_name.unwrap_or_else(|| creator_id.to_string()),
            _ => creator_id.to_string(),
        }
    }
}

/// Load the CLI config snapshot, mapping read/parse failures onto the core
/// taxonomy.
fn load_config_snapshot(nexus_home: &Path) -> CoreResult<CliConfigSnapshot> {
    CliConfigSnapshot::load(nexus_home).map_err(|e| CoreError::Internal {
        category: format!("config_load: {e}"),
    })
}

/// Validate the optional display name at the identity front door — the same
/// rules as the CLI helper: non-empty after trim, no control characters, and
/// at most [`MAX_CREATOR_NAME_LENGTH`] bytes. Returns the trimmed name.
fn validated_display_name(name: Option<&str>) -> CoreResult<Option<String>> {
    let Some(raw) = name else {
        return Ok(None);
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(CoreError::InvalidInput {
            field: "display_name".to_string(),
            reason: "display name cannot be empty or whitespace-only".to_string(),
        });
    }
    if trimmed.chars().any(char::is_control) {
        return Err(CoreError::InvalidInput {
            field: "display_name".to_string(),
            reason: "display name cannot contain control characters".to_string(),
        });
    }
    if trimmed.len() > MAX_CREATOR_NAME_LENGTH {
        return Err(CoreError::InvalidInput {
            field: "display_name".to_string(),
            reason: format!(
                "display name exceeds maximum length ({MAX_CREATOR_NAME_LENGTH} bytes)"
            ),
        });
    }
    Ok(Some(trimmed.to_string()))
}

/// Persistent identity rows whose display name is byte-exactly `trimmed`
/// (`str::trim` + `==`: no case-fold, no normalization).
fn persistent_rows_with_name(
    rows: &[nexus_local_db::LocalIdentityRow],
    trimmed: &str,
) -> Vec<nexus_local_db::LocalIdentityRow> {
    rows.iter()
        .filter(|r| r.identity_type == "persistent" && r.display_name.as_deref() == Some(trimmed))
        .cloned()
        .collect()
}

fn collision_error(display: &str, matches: &[nexus_local_db::LocalIdentityRow]) -> CoreError {
    let ids: Vec<&str> = matches.iter().map(|r| r.creator_id.as_str()).collect();
    CoreError::InvalidInput {
        field: "display_name".to_string(),
        reason: format!(
            "creator name collision: {display} is already used by persistent identities {ids:?}"
        ),
    }
}

/// Mint a persistent local creator id — `ctr_local` + 12 hex chars from a
/// UUID v4. The pattern is the public contract (same inline copy the daemon
/// `create_creator` handler uses; the private helper is not cross-crate).
fn mint_local_creator_id() -> String {
    let random: String = uuid::Uuid::new_v4()
        .to_string()
        .replace('-', "")
        .chars()
        .take(12)
        .collect();
    format!("ctr_local{random}")
}

/// Validate a workspace slug: non-empty, single path segment, no `.` / `..`,
/// no control characters (retained daemon `validate_slug` rule plus the shared
/// name-safety rule, core error type).
fn validate_workspace_slug(slug: &str) -> CoreResult<()> {
    let reason = if slug.is_empty()
        || slug.contains('/')
        || slug.contains('\\')
        || slug == "."
        || slug == ".."
    {
        "must be a single path segment"
    } else if slug.chars().any(char::is_control) {
        "must not contain control characters"
    } else {
        return Ok(());
    };
    Err(CoreError::InvalidInput {
        field: "workspace_slug".to_string(),
        reason: reason.to_string(),
    })
}

/// The creative root of a workspace: only the first workspace (`default`)
/// honours the legacy setup root for the creator; every additional workspace
/// is `~/Nexus/<slug>`. This is the ONE derivation rule shared by the
/// onboarding selection path and `create_workspace` (access spec §3.1 Q3).
/// A relative legacy path resolves against the raw user home.
fn default_creative_root(
    user_home: &Path,
    nexus_home: &Path,
    creator_id: &str,
    slug: &str,
) -> PathBuf {
    if slug != "default" {
        return user_home.join("Nexus").join(slug);
    }
    let legacy = crate::creators::read_cli_config(nexus_home)
        .ok()
        .and_then(|config| {
            config
                .get("workspace_path_by_creator")
                .and_then(|paths| paths.get(creator_id))
                .and_then(toml::Value::as_str)
                .or_else(|| config.get("workspace_path").and_then(toml::Value::as_str))
                .map(PathBuf::from)
        });
    match legacy {
        Some(path) if path.is_absolute() => path,
        Some(path) => user_home.join(path),
        None => user_home.join("Nexus"),
    }
}

/// Reject a creative-root override that is not absolute, traverses with `..`,
/// or carries control characters — the `validate_workspace_path_safe` safety
/// semantics, applied to an absolute create target.
fn validate_creative_root_override(raw: &str) -> CoreResult<()> {
    let path = Path::new(raw);
    let reason = if !path.is_absolute() {
        "must be an absolute path"
    } else if path
        .components()
        .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        "must not contain '..'"
    } else if raw.chars().any(char::is_control) {
        "must not contain control characters"
    } else {
        return Ok(());
    };
    Err(CoreError::InvalidInput {
        field: "creative_root".to_string(),
        reason: reason.to_string(),
    })
}

/// True when `dir` has no entries (a freshly chosen empty directory is adopted).
/// An unreadable directory is a client-side root problem, so it folds to
/// `invalid_input` rather than an internal fault.
fn directory_is_empty(dir: &Path) -> CoreResult<bool> {
    let mut entries = std::fs::read_dir(dir).map_err(|_| CoreError::InvalidInput {
        field: "creative_root".to_string(),
        reason: "not readable".to_string(),
    })?;
    Ok(entries.next().is_none())
}

/// Prove the creative root is writable before any registration artifact is
/// produced: create it with parents, then write/remove a probe file. The probe
/// removes its own file on a failed write; the caller removes a root this create
/// created when the probe fails.
fn probe_creative_dir(root: &Path) -> CoreResult<()> {
    let probe = root.join(format!(".nexus42-probe-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(root)
        .and_then(|()| std::fs::write(&probe, b"probe"))
        .and_then(|()| std::fs::remove_file(&probe))
        .map_err(|_| {
            let _ = std::fs::remove_file(&probe);
            CoreError::InvalidInput {
                field: "creative_root".to_string(),
                reason: "not writable".to_string(),
            }
        })
}

/// Undo a failed create by removing only the artifacts this call produced.
///
/// The operational directory is removed recursively only when this call created
/// it; a pre-existing directory keeps everything else. An adopted creative root
/// is never deleted — only the manifest file this call wrote (and the
/// `.nexus42` directory when that leaves it empty). `meta.json` is the
/// registration marker and never pre-existed here (the preflight refuses a
/// registered slug), so a partial write of it is safe to drop.
#[allow(clippy::too_many_arguments)] // explicit artifact bookkeeping, one call site
fn rollback_new_workspace(
    op_dir: &Path,
    op_dir_preexisting: bool,
    db_path: &Path,
    db_preexisting: bool,
    manifest_path: &Path,
    manifest_preexisting: bool,
    creative_root: &Path,
    root_preexisting: bool,
) {
    if !db_preexisting {
        let _ = std::fs::remove_file(db_path);
    }
    if !manifest_preexisting {
        let _ = std::fs::remove_file(manifest_path);
    }
    let _ = std::fs::remove_file(op_dir.join("meta.json"));
    if !op_dir_preexisting {
        let _ = std::fs::remove_dir_all(op_dir);
    }
    if root_preexisting {
        let meta_dir = creative_root.join(".nexus42");
        let empty = std::fs::read_dir(&meta_dir)
            .map(|mut entries| entries.next().is_none())
            .unwrap_or(false);
        if empty {
            let _ = std::fs::remove_dir(&meta_dir);
        }
    } else {
        let _ = std::fs::remove_dir_all(creative_root);
    }
}

/// Pretty-print `value` into `path`; the category names the failing artifact.
fn write_json_file(path: &Path, value: &serde_json::Value, category: &str) -> CoreResult<()> {
    let text = serde_json::to_string_pretty(value).map_err(|e| CoreError::Internal {
        category: format!("{category}_serialize: {e}"),
    })?;
    std::fs::write(path, text).map_err(|e| CoreError::Internal {
        category: format!("{category}_write: {e}"),
    })
}

/// Deny selecting a workspace whose operational `meta.json` registers a
/// different creator or slug (foreign-workspace denial). A missing or
/// unparseable `meta.json` keeps the retained existence-only semantics.
fn deny_foreign_workspace(op_dir: &Path, creator_id: &str, slug: &str) -> CoreResult<()> {
    let Ok(meta) = std::fs::read_to_string(op_dir.join("meta.json")) else {
        return Ok(());
    };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&meta) else {
        return Ok(());
    };
    let meta_creator = json.get("creator_id").and_then(serde_json::Value::as_str);
    let meta_slug = json
        .get("workspace_slug")
        .and_then(serde_json::Value::as_str);
    let foreign =
        meta_creator.is_some_and(|mc| mc != creator_id) || meta_slug.is_some_and(|ms| ms != slug);
    if foreign {
        return Err(CoreError::NotFound {
            resource: format!("Workspace {slug} does not exist for creator {creator_id}"),
        });
    }
    Ok(())
}

/// Scan all creator directories under `nexus_home` and collect workspace
/// registrations (ported from the daemon handler). Sorted by
/// `(creator_id, workspace_slug)`; only workspaces with a `meta.json` count.
#[must_use]
pub fn scan_workspaces(nexus_home: &Path) -> Vec<NexusWorkspaceSummary> {
    let creators_root = nexus_home.join("creators");
    let mut items = Vec::new();

    let Ok(creator_entries) = std::fs::read_dir(&creators_root) else {
        return items;
    };

    for creator_entry in creator_entries.flatten() {
        if !creator_entry.path().is_dir() {
            continue;
        }
        let Ok(creator_id) = creator_entry.file_name().into_string() else {
            continue;
        };
        let ws_root = creators_root.join(&creator_id).join("workspaces");
        let Ok(ws_entries) = std::fs::read_dir(&ws_root) else {
            continue;
        };

        for ws_entry in ws_entries.flatten() {
            if !ws_entry.path().is_dir() {
                continue;
            }
            let Ok(slug) = ws_entry.file_name().into_string() else {
                continue;
            };

            let op_dir = ws_entry.path();
            if !op_dir.join("meta.json").exists() {
                continue;
            }

            let creative_root = read_meta_creative_root(&op_dir).unwrap_or_default();
            let display_name = read_workspace_display_name(Path::new(&creative_root));

            items.push(NexusWorkspaceSummary {
                creator_id: creator_id.clone(),
                workspace_slug: slug,
                creative_root,
                display_name,
            });
        }
    }

    items.sort_by(|a, b| {
        a.creator_id
            .cmp(&b.creator_id)
            .then(a.workspace_slug.cmp(&b.workspace_slug))
    });

    items
}

/// Read `creative_root` from operational `meta.json` (ported from the daemon
/// handler). Returns `None` if the file doesn't exist or can't be parsed.
#[must_use]
pub fn read_meta_creative_root(op_dir: &Path) -> Option<String> {
    let content = std::fs::read_to_string(op_dir.join("meta.json")).ok()?;
    let json: serde_json::Value = serde_json::from_str(&content).ok()?;
    json.get("local_root")
        .and_then(serde_json::Value::as_str)
        .map(std::string::ToString::to_string)
}

/// Read `display_name` from `.nexus42/workspace.json` in the creative root
/// (ported from the daemon handler). Returns `None` if the file doesn't exist
/// or can't be parsed.
#[must_use]
pub fn read_workspace_display_name(creative_root: &Path) -> Option<String> {
    let content =
        std::fs::read_to_string(creative_root.join(".nexus42").join("workspace.json")).ok()?;
    let json: serde_json::Value = serde_json::from_str(&content).ok()?;
    json.get("name")
        .and_then(serde_json::Value::as_str)
        .map(std::string::ToString::to_string)
}

/// Write the active `creator_id` — and, when `workspace_slug` is `Some`, the
/// per-creator active workspace slug — to `config.toml`, preserving every
/// other key (ported from the daemon `write_active_selection`).
pub fn write_active_selection(
    nexus_home: &Path,
    creator_id: &str,
    workspace_slug: Option<&str>,
) -> CoreResult<()> {
    let config_path = nexus_home.join("config.toml");

    let mut config: toml::Table = if config_path.exists() {
        let content = std::fs::read_to_string(&config_path).map_err(|e| CoreError::Internal {
            category: format!("config_read: {e}"),
        })?;
        if content.trim().is_empty() {
            toml::Table::new()
        } else {
            content.parse().map_err(|e| CoreError::Internal {
                category: format!("config_parse: {e}"),
            })?
        }
    } else {
        toml::Table::new()
    };

    config.insert(
        "active_creator_id".to_string(),
        toml::Value::String(creator_id.to_string()),
    );

    if let Some(workspace_slug) = workspace_slug {
        let Some(slug_table) = config
            .entry("active_workspace_slug_by_creator")
            .or_insert_with(|| toml::Value::Table(toml::Table::new()))
            .as_table_mut()
        else {
            return Err(CoreError::Internal {
                category: "config: active_workspace_slug_by_creator is not a table".to_string(),
            });
        };
        slug_table.insert(
            creator_id.to_string(),
            toml::Value::String(workspace_slug.to_string()),
        );
    }

    let toml_str = toml::to_string_pretty(&config).map_err(|e| CoreError::Internal {
        category: format!("config_serialize: {e}"),
    })?;
    std::fs::write(&config_path, toml_str).map_err(|e| CoreError::Internal {
        category: format!("config_write: {e}"),
    })?;

    Ok(())
}

/// Build a generated string newtype from a plain `str`; the generated
/// newtypes validate their own bounds (e.g. non-emptiness), so a violating
/// stored value is an honest internal failure.
pub fn newtype_value<T>(value: &str) -> CoreResult<T>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    value.parse::<T>().map_err(|e| CoreError::Internal {
        category: format!("field_newtype: {e}"),
    })
}
