//! Connect Host peer allowlist (`~/.nexus42/connect/allowlist.json` +
//! repeatable `--allow-peer` overlay).
//!
//! N-C0 product contract (draft §2.3): the allowlist is the trust root.
//! N-C1 → N-C2 world scoping (P1 spec § World scoping — schema locked):
//! each `peer_ids` entry is either a bare `"12D3…"` peer id (N-C0 shape —
//! no op access) or an object `{ "peer_id": "12D3…", "world_scope":
//! ["<world-uuid>", …], "op_scope": ["upsert","promote","relate","check",
//! "assemble", "tools.nexus.<tool-id>", …], "module_scope": ["<module-id>", …] }`.
//! V1.173 (DF-84, AR-49): `op_scope` may list `tools.nexus.*` op strings —
//! they are exact-membership entries like any other served op.
//! All scopes are optional and **fail-closed**: an absent/empty scope
//! denies world access (writes AND the world-scoped read ops), ops, and —
//! since P2 — every compute module (the `module_scope` architect lock,
//! spec §6.1: missing/empty ⇒ deny ALL compute). A bare entry (or a
//! `--allow-peer` overlay) is handshake-allowlisted but can never invoke a
//! served op. World ids are world UUID strings, never filesystem paths;
//! module ids are host-local module names (never peer-supplied bytes). A
//! missing file ⇒ empty list ⇒ **fail-closed** (spoke-connect rejects every
//! remote peer). The operator edits the allowlist out-of-band; there is no
//! online enroll endpoint.
//!
//! V1.191 P1 T14 (`holder-governance.md` §9) adds the **Actor grant**: an
//! optional per-peer `"grant"` object naming the *stored* Actor the peer
//! acts as (`creator_id` + `actor` ActorRef) and its World/binding
//! viewpoint. The grant is the operator's pointer at stored state — it is
//! never carried on a request, and every KE invoke re-resolves it against
//! current stored ownership/lifecycle ([`PeerGrant::resolve_scope`]): a
//! peer with no grant, or one whose stored ownership/holder row no longer
//! resolves, performs no KE operation. The allowlist row's peer id IS the
//! granted peer, so the authenticated session peer can only ever reach its
//! own grant; `world_scope`/`op_scope`/`module_scope` remain independent
//! gates that must *also* pass (grant ∩ scopes ∩ capability token).
//!

use crate::errors::{CliError, Result};
use libp2p::PeerId;
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// On-disk allowlist shape (`allowlist.json`).
///
/// `deny_unknown_fields` turns typos like `peerIds` / `peer-id` into a hard
/// config error instead of silently producing an empty (fail-closed)
/// allowlist that confuses the operator. The same guard applies inside each
/// scoped entry ([`PeerEntryScoped`]).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AllowlistFile {
    peer_ids: Vec<PeerEntry>,
}

/// One `peer_ids` entry: a bare peer id (N-C0 shape — no op access) or a
/// scoped object (N-C1 → N-C2).
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum PeerEntry {
    Bare(String),
    Scoped(PeerEntryScoped),
}

/// Scoped entry form — locked schema (P1 spec § World scoping; P2 spec
/// §6.1 adds `module_scope`).
///
/// `world_scope` / `op_scope` / `module_scope` are optional; absent fields
/// deserialize to empty lists and the gate then denies every world/op/
/// module (fail-closed).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PeerEntryScoped {
    peer_id: String,
    #[serde(default)]
    world_scope: Vec<String>,
    #[serde(default)]
    op_scope: Vec<String>,
    /// Host-local compute module ids this peer may invoke (P2 — architect
    /// lock, spec §6.1: absent/empty denies ALL compute).
    #[serde(default)]
    module_scope: Vec<String>,
    /// Operator-stored Actor grant (v1.191 P1 T14, durable §9). Absent ⇒ the
    /// peer is handshake/scope-allowlisted but performs no KE operation.
    #[serde(default)]
    grant: Option<PeerGrant>,
}

/// Operator-stored Actor grant for one allowlisted peer (v1.191 P1 T14 —
/// durable `holder-governance.md` §9).
///
/// The grant names the **stored** Actor the peer acts as — a controlling
/// Creator plus the ActorRef (that Creator, or one of its Characters) — and
/// the World/binding viewpoint it is admitted at. It is persisted in the
/// operator's `allowlist.json` and never travels on a request; each
/// authenticated KE invoke re-resolves it against current stored state
/// ([`PeerGrant::resolve_scope`]), so a revoked/renamed/foreign subject
/// fails closed on the next invoke. Raw request selectors (`owner`,
/// `scope.viewpoint`, containers) can only narrow/match this grant — they
/// can never choose another Actor or a management selection.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PeerGrant {
    /// Controlling Creator of the granted Actor: the stored owner whose
    /// ownership rows authorize the containers (`actor_view_scope` resolves
    /// them server-side, never from the request).
    pub creator_id: String,
    /// The granted ActorRef — the peer acts as this stored identity only.
    pub actor: GrantActor,
    /// Granted World (world UUID string, never a path).
    pub world_id: String,
    /// Granted Actor-World binding; REQUIRED for a Character grant (the
    /// Character viewpoint is World + Character + binding).
    #[serde(default)]
    pub binding_id: Option<String>,
}

/// The ActorRef half of a [`PeerGrant`] — mirrors `nexus_core::AdmittedActor`
/// without importing it into the on-disk schema.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum GrantActor {
    /// The controlling Creator itself.
    Creator {
        /// Creator id (must equal [`PeerGrant::creator_id`]).
        id: String,
    },
    /// One Character owned by the controlling Creator.
    Character {
        /// Character id.
        id: String,
    },
}

impl PeerGrant {
    /// The granted World viewpoint.
    #[must_use]
    pub fn world_id(&self) -> &str {
        &self.world_id
    }

    /// The granted ActorRef id (Creator or Character id).
    #[must_use]
    pub fn actor_id(&self) -> &str {
        match &self.actor {
            GrantActor::Creator { id } | GrantActor::Character { id } => id,
        }
    }

    /// Fail-closed validation of one operator-authored grant.
    ///
    /// A grant is a pointer at stored state, so a malformed one must be a
    /// hard config error rather than a row that denies every invoke later:
    /// empty ids are refused, a Creator grant whose ActorRef names a
    /// different subject is refused (the durable §9 pair is "controlling
    /// Creator + ActorRef", and two different subjects would silently deny
    /// every invoke), and a Character grant without a binding is refused
    /// (the Character viewpoint is World + Character + binding).
    fn validate(&self) -> Result<()> {
        if self.creator_id.is_empty() {
            return Err(CliError::Config(
                "grant.creator_id must be a nonempty Creator id".to_string(),
            ));
        }
        if self.world_id.is_empty() {
            return Err(CliError::Config(
                "grant.world_id must be a nonempty World id".to_string(),
            ));
        }
        if self.actor_id().is_empty() {
            return Err(CliError::Config(
                "grant.actor.id must be a nonempty Actor id".to_string(),
            ));
        }
        match &self.actor {
            GrantActor::Creator { id } if id != &self.creator_id => Err(CliError::Config(format!(
                "grant names Creator {id:?} as its actor but Creator {:?} as its controlling \
                 creator; a Creator grant's actor is its controlling Creator",
                self.creator_id
            ))),
            GrantActor::Creator { .. } => Ok(()),
            GrantActor::Character { .. } => match &self.binding_id {
                Some(binding) if !binding.is_empty() => Ok(()),
                _ => Err(CliError::Config(
                    "grant.binding_id is required for a Character grant (the Character \
                     viewpoint is World + Character + binding)"
                        .to_string(),
                )),
            },
        }
    }

    /// Resolve this grant against **current stored ownership/lifecycle** into
    /// the admitted read selection it authorizes (v1.191 P1 T14, durable §4.1
    /// / §9).
    ///
    /// The resolution is the core admission authority
    /// (`ActorKnowledgeViewService::actor_view_scope`): the containers come
    /// from the stored ownership rows of `creator_id`, the admitted holder
    /// from the stored identity's registry entry, and a foreign/missing
    /// subject or a missing registry row fails closed. It is called on every
    /// KE invoke, never cached — a grant that no longer resolves serves
    /// nothing.
    ///
    /// The result is always an `ActorView` selection: a Creator grant does
    /// NOT become a `CreatorManagement` review (durable §9 — no grant
    /// inherits management review).
    ///
    /// # Errors
    /// Returns [`CliError::Other`] when the stored Actor no longer resolves
    /// (revoked/stale grant, foreign or missing subject, missing holder
    /// registry row).
    pub async fn resolve_scope(
        &self,
        pool: &sqlx::SqlitePool,
    ) -> Result<nexus_knowledge::world_kb::KnowledgeReadScope> {
        let actor = match &self.actor {
            GrantActor::Creator { id } => nexus_core::AdmittedActor::Creator {
                creator_id: id.clone(),
            },
            GrantActor::Character { id } => nexus_core::AdmittedActor::Character {
                character_id: id.clone(),
            },
        };
        nexus_core::ActorKnowledgeViewService::new(pool.clone())
            .actor_view_scope(
                &self.creator_id,
                &actor,
                &self.world_id,
                self.binding_id.as_deref(),
            )
            .await
            .map_err(|e| {
                CliError::Other(format!(
                    "stored Actor grant (creator {}, actor {}, world {}) does not resolve: {e}",
                    self.creator_id,
                    self.actor_id(),
                    self.world_id
                ))
            })
    }
}

/// Resolved allowlist scoping: peer → allowed world ids / allowed ops.
///
/// Consumed by (a) the N-C1 dispatch gate via [`PeerScope::allows_world`] /
/// [`PeerScope::allows_op`] and (b) `ConnectConfig.peer_allowlist` via
/// [`PeerScope::peer_ids`] (handshake allow set).
#[derive(Debug, Clone, Default)]
pub struct PeerScope {
    entries: BTreeMap<PeerId, PeerAccess>,
}

/// Per-peer scoping. Empty sets ⇒ fail-closed: the peer is handshake-
/// allowlisted but has no world/op/module access.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PeerAccess {
    /// World ids (world UUID strings, not paths) this peer may target —
    /// writes AND the world-scoped read ops (`check` / `assemble`) and the
    /// P2 compute op share the same gate.
    pub world_scope: BTreeSet<String>,
    /// Ops this peer may invoke (N-C2 E2 served ops: `upsert` / `promote` /
    /// `relate` / `check` / `assemble` / `compute`). V1.173 (DF-84, AR-49):
    /// may also list `tools.nexus.*` strings (e.g.
    /// `tools.nexus.list_observed_peers`) — they flow through the same
    /// exact-membership [`PeerScope::allows_op`] gate as the core ops, so
    /// an operator must add each served tool id the peer may call, or the
    /// invoke is denied after the `SERVED_OPS` gate.
    pub op_scope: BTreeSet<String>,
    /// Host-local compute module ids this peer may invoke (P2 — architect
    /// lock, spec §6.1: missing/empty denies ALL compute, fail-closed).
    /// Module ids are operator-allowlisted names; the module bytes are
    /// never peer-supplied (resolved host-locally under
    /// `~/.nexus42/modules/` only).
    pub module_scope: BTreeSet<String>,
    /// Operator-stored Actor grant (v1.191 P1 T14, durable §9). `None` ⇒ no
    /// KE operation is served to this peer, whatever its scopes say.
    pub grant: Option<PeerGrant>,
}

impl PeerScope {
    /// All allowlisted peer ids (sorted) — the `ConnectConfig.peer_allowlist`
    /// handshake set.
    #[must_use]
    pub fn peer_ids(&self) -> Vec<PeerId> {
        self.entries.keys().copied().collect()
    }

    /// Per-peer access; `None` when the peer is not allowlisted.
    #[must_use]
    pub fn access_for(&self, peer: &PeerId) -> Option<&PeerAccess> {
        self.entries.get(peer)
    }

    /// The stored Actor grant of one peer (v1.191 P1 T14, durable §9).
    ///
    /// Keyed by the **authenticated session peer**: the lookup is the
    /// grant's peer binding, so a peer can only ever reach its own grant —
    /// never another actor's, and never one named by a request payload.
    /// `None` ⇒ no KE operation is served to this peer.
    #[must_use]
    pub fn grant_for(&self, peer: &PeerId) -> Option<&PeerGrant> {
        self.access_for(peer).and_then(|access| access.grant.as_ref())
    }

    /// Fail-closed world gate: true only when the peer is allowlisted AND its
    /// `world_scope` contains `world_id`.
    #[must_use]
    pub fn allows_world(&self, peer: &PeerId, world_id: &str) -> bool {
        self.access_for(peer)
            .is_some_and(|access| access.world_scope.contains(world_id))
    }

    /// Fail-closed op gate: true only when the peer is allowlisted AND its
    /// `op_scope` contains `op`.
    #[must_use]
    pub fn allows_op(&self, peer: &PeerId, op: &str) -> bool {
        self.access_for(peer)
            .is_some_and(|access| access.op_scope.contains(op))
    }

    /// Fail-closed compute-module gate (P2 architect lock, spec §6.1): true
    /// only when the peer is allowlisted AND its `module_scope` contains
    /// `module_id`. An absent/empty `module_scope` denies ALL compute.
    #[must_use]
    pub fn allows_module(&self, peer: &PeerId, module_id: &str) -> bool {
        self.access_for(peer)
            .is_some_and(|access| access.module_scope.contains(module_id))
    }

    /// True when no peer is allowlisted at all (missing/empty file).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// File entries: last occurrence wins (a later hand-authored duplicate
    /// overrides an earlier one, matching JSON duplicate-key conventions).
    fn insert_entry(&mut self, entry: PeerEntry) -> Result<()> {
        match entry {
            PeerEntry::Bare(id) => {
                let peer = parse_peer_id(&id)?;
                self.entries.insert(peer, PeerAccess::default());
            }
            PeerEntry::Scoped(scoped) => {
                let peer = parse_peer_id(&scoped.peer_id)?;
                if let Some(grant) = &scoped.grant {
                    grant.validate()?;
                }
                self.entries.insert(
                    peer,
                    PeerAccess {
                        world_scope: scoped.world_scope.into_iter().collect(),
                        op_scope: scoped.op_scope.into_iter().collect(),
                        module_scope: scoped.module_scope.into_iter().collect(),
                        grant: scoped.grant,
                    },
                );
            }
        }
        Ok(())
    }

    /// CLI overlay: fills gaps only — a `--allow-peer` entry (which carries
    /// no scope) must never strip a hand-authored file scope for the same
    /// peer. CLI peers have no write access by design (fail-closed).
    fn insert_cli_peer(&mut self, id: &str) -> Result<()> {
        let peer = parse_peer_id(id)?;
        self.entries.entry(peer).or_default();
        Ok(())
    }
}

fn parse_peer_id(raw: &str) -> Result<PeerId> {
    raw.parse::<PeerId>()
        .map_err(|e| CliError::Config(format!("invalid peer id {raw:?} in allowlist: {e}")))
}

/// Load the effective allowlist: file entries ∪ `--allow-peer` CLI entries.
///
/// A missing file is not an error — it contributes nothing (fail-closed).
/// An unreadable/malformed file or an unparseable peer id is a hard error so
/// a typo cannot silently open or lock the host.
///
/// # Parameters
/// `home` is the **raw user home** (`$HOME`); this fn joins `.nexus42`
/// internally via `connect_allowlist_path`, so callers MUST NOT pre-join
/// `~/.nexus42`.
///
/// # Returns
/// A [`PeerScope`]: `peer_ids()` feeds `ConnectConfig.peer_allowlist`
/// (handshake), `allows_world` / `allows_op` feed the N-C1 dispatch gate.
/// CLI overlay entries carry no scope — write access requires a
/// hand-authored `allowlist.json` scope (fail-closed).
///
/// # Errors
/// Returns [`CliError::Io`] when the file exists but cannot be read, or
/// [`CliError::Config`] when the file is malformed or an entry is not a
/// valid libp2p `PeerId`.
pub fn load(home: &Path, cli_peers: &[String]) -> Result<PeerScope> {
    let path = nexus_home_layout::connect_allowlist_path(home);

    let mut scope = PeerScope::default();
    match std::fs::read_to_string(&path) {
        Ok(content) => {
            let parsed: AllowlistFile = serde_json::from_str(&content).map_err(|e| {
                CliError::Config(format!("invalid allowlist at {}: {e}", path.display()))
            })?;
            for entry in parsed.peer_ids {
                scope.insert_entry(entry)?;
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // Missing file ⇒ empty allowlist ⇒ fail-closed.
        }
        Err(e) => return Err(CliError::Io(e)),
    }
    for peer in cli_peers {
        scope.insert_cli_peer(peer)?;
    }
    Ok(scope)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn peer_id(seed: u8) -> PeerId {
        libp2p::identity::Keypair::ed25519_from_bytes([seed; 32])
            .expect("seed is a valid ed25519 secret")
            .public()
            .to_peer_id()
    }

    fn write_allowlist(home: &Path, peer_ids: &serde_json::Value) {
        let path = nexus_home_layout::connect_allowlist_path(home);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        std::fs::write(
            &path,
            serde_json::json!({ "peer_ids": peer_ids }).to_string(),
        )
        .expect("write allowlist");
    }

    #[test]
    fn missing_file_is_empty_and_fail_closed() {
        let temp = tempfile::tempdir().expect("tempdir");
        let scope = load(temp.path(), &[]).expect("missing file loads as empty");
        assert!(
            scope.is_empty(),
            "missing allowlist file must resolve to an empty (fail-closed) allowlist"
        );
    }

    #[test]
    fn cli_peers_overlay_on_empty_file() {
        let temp = tempfile::tempdir().expect("tempdir");
        let peer = peer_id(1);
        let scope = load(temp.path(), &[peer.to_string()]).expect("cli peer loads");
        assert_eq!(scope.peer_ids(), vec![peer]);
    }

    #[test]
    fn file_entries_are_unioned_with_cli_peers() {
        let temp = tempfile::tempdir().expect("tempdir");
        let file_peer = peer_id(2);
        let cli_peer = peer_id(3);
        write_allowlist(temp.path(), &serde_json::json!([file_peer.to_string()]));

        let scope = load(temp.path(), &[cli_peer.to_string()]).expect("load");
        let mut expected = vec![file_peer, cli_peer];
        expected.sort();
        assert_eq!(scope.peer_ids(), expected);
    }

    #[test]
    fn malformed_file_is_a_config_error() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = nexus_home_layout::connect_allowlist_path(temp.path());
        std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        std::fs::write(&path, "{ not json").expect("write");

        let err = load(temp.path(), &[]).expect_err("malformed allowlist rejected");
        assert!(matches!(err, CliError::Config(_)), "got {err:?}");
    }

    #[test]
    fn unknown_allowlist_fields_are_rejected() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = nexus_home_layout::connect_allowlist_path(temp.path());
        std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        // `peerIds` is a common typo for `peer_ids`; accepting it would
        // silently produce an empty (fail-closed) allowlist.
        std::fs::write(&path, r#"{ "peer_ids": [], "peerIds": ["12D3KooWxxxx"] }"#).expect("write");

        let err = load(temp.path(), &[]).expect_err("unknown field rejected");
        assert!(matches!(err, CliError::Config(_)), "got {err:?}");
    }

    #[test]
    fn invalid_peer_id_is_a_config_error() {
        let temp = tempfile::tempdir().expect("tempdir");
        let err =
            load(temp.path(), &["not-a-peer-id".into()]).expect_err("invalid peer id rejected");
        assert!(matches!(err, CliError::Config(_)), "got {err:?}");
    }

    // ---- N-C1 world/op scoping (locked schema, fail-closed) ----

    /// The T1 gate contract (brief Step 1): a scoped peer is allowed on a
    /// listed world and denied on every other world; ops outside its
    /// `op_scope` are denied too.
    #[test]
    fn scoped_peer_allowed_on_listed_world_denied_on_other_world() {
        let temp = tempfile::tempdir().expect("tempdir");
        let peer = peer_id(4);
        write_allowlist(
            temp.path(),
            &serde_json::json!([{
                "peer_id": peer.to_string(),
                "world_scope": ["world-a"],
                "op_scope": ["upsert", "promote"],
            }]),
        );

        let scope = load(temp.path(), &[]).expect("scoped entry loads");
        assert_eq!(
            scope.peer_ids(),
            vec![peer],
            "scoped peer stays allowlisted"
        );
        assert!(
            scope.allows_world(&peer, "world-a"),
            "peer must be allowed on its listed world"
        );
        assert!(
            !scope.allows_world(&peer, "world-b"),
            "peer must be denied on an unlisted world"
        );
        assert!(scope.allows_op(&peer, "upsert"), "listed op allowed");
        assert!(scope.allows_op(&peer, "promote"), "listed op allowed");
        assert!(!scope.allows_op(&peer, "relate"), "unlisted op denied");
    }

    /// N-C0 backward compat: a bare string entry stays handshake-allowlisted
    /// but carries no scope ⇒ never any world-write access (fail-closed).
    #[test]
    fn bare_string_entries_are_allowlisted_but_never_write() {
        let temp = tempfile::tempdir().expect("tempdir");
        let peer = peer_id(5);
        write_allowlist(temp.path(), &serde_json::json!([peer.to_string()]));

        let scope = load(temp.path(), &[]).expect("bare entry loads");
        assert_eq!(scope.peer_ids(), vec![peer], "bare entry stays allowlisted");
        assert!(
            !scope.allows_world(&peer, "world-a"),
            "bare entry has no world scope — fail-closed"
        );
        assert!(
            !scope.allows_op(&peer, "upsert"),
            "bare entry has no op scope — fail-closed"
        );
        assert!(
            !scope.allows_module(&peer, "basic-combat"),
            "bare entry has no module scope — fail-closed"
        );
    }

    /// Absent `world_scope` ⇒ no world writes, even with an op scope present.
    #[test]
    fn object_entry_without_world_scope_is_fail_closed() {
        let temp = tempfile::tempdir().expect("tempdir");
        let peer = peer_id(6);
        write_allowlist(
            temp.path(),
            &serde_json::json!([{ "peer_id": peer.to_string(), "op_scope": ["upsert"] }]),
        );

        let scope = load(temp.path(), &[]).expect("entry without world_scope loads");
        assert!(
            !scope.allows_world(&peer, "world-a"),
            "absent world_scope must deny every world"
        );
    }

    /// Absent `op_scope` ⇒ no ops, even with a world scope present.
    #[test]
    fn object_entry_without_op_scope_is_fail_closed() {
        let temp = tempfile::tempdir().expect("tempdir");
        let peer = peer_id(7);
        write_allowlist(
            temp.path(),
            &serde_json::json!([{ "peer_id": peer.to_string(), "world_scope": ["world-a"] }]),
        );

        let scope = load(temp.path(), &[]).expect("entry without op_scope loads");
        assert!(
            !scope.allows_op(&peer, "upsert"),
            "absent op_scope must deny every op"
        );
    }

    // ---- N-C2 (P2) module scoping (architect lock, spec §6.1) ----

    /// The P2 module gate contract: a scoped peer is allowed to invoke a
    /// listed module and denied every other module.
    #[test]
    fn scoped_peer_allowed_on_listed_module_denied_on_other_modules() {
        let temp = tempfile::tempdir().expect("tempdir");
        let peer = peer_id(14);
        write_allowlist(
            temp.path(),
            &serde_json::json!([{
                "peer_id": peer.to_string(),
                "world_scope": ["world-a"],
                "op_scope": ["compute"],
                "module_scope": ["basic-combat"],
            }]),
        );

        let scope = load(temp.path(), &[]).expect("scoped entry loads");
        assert!(
            scope.allows_module(&peer, "basic-combat"),
            "peer must be allowed on its listed module"
        );
        assert!(
            !scope.allows_module(&peer, "another-module"),
            "peer must be denied on an unlisted module"
        );
    }

    /// Absent `module_scope` ⇒ no compute, even with world/op scopes
    /// present (the architect lock: missing or empty scope denies ALL
    /// compute — fail-closed). Also the backward-compat pin: a V1.153 →
    /// V1.154 allowlist file WITHOUT the new field still loads (optional
    /// field) and simply carries no module access.
    #[test]
    fn object_entry_without_module_scope_is_fail_closed() {
        let temp = tempfile::tempdir().expect("tempdir");
        let peer = peer_id(15);
        write_allowlist(
            temp.path(),
            &serde_json::json!([{
                "peer_id": peer.to_string(),
                "world_scope": ["world-a"],
                "op_scope": ["compute"],
            }]),
        );

        let scope = load(temp.path(), &[]).expect("entry without module_scope loads");
        assert!(
            !scope.allows_module(&peer, "basic-combat"),
            "absent module_scope must deny every module (fail-closed)"
        );
    }

    /// An explicit empty `module_scope` list denies compute exactly like an
    /// absent field.
    #[test]
    fn empty_module_scope_is_fail_closed() {
        let temp = tempfile::tempdir().expect("tempdir");
        let peer = peer_id(16);
        write_allowlist(
            temp.path(),
            &serde_json::json!([{
                "peer_id": peer.to_string(),
                "world_scope": ["world-a"],
                "op_scope": ["compute"],
                "module_scope": [],
            }]),
        );

        let scope = load(temp.path(), &[]).expect("empty module_scope loads");
        assert!(
            !scope.allows_module(&peer, "basic-combat"),
            "an empty module_scope must deny every module"
        );
    }

    /// `deny_unknown_fields` is retained on the scoped object form: a typo
    /// like `op_scop` stays a hard config error.
    #[test]
    fn unknown_fields_inside_scoped_entry_are_rejected() {
        let temp = tempfile::tempdir().expect("tempdir");
        write_allowlist(
            temp.path(),
            &serde_json::json!([{
                "peer_id": peer_id(8).to_string(),
                "world_scope": ["world-a"],
                "op_scop": ["upsert"],
            }]),
        );

        let err = load(temp.path(), &[]).expect_err("unknown scoped field rejected");
        assert!(matches!(err, CliError::Config(_)), "got {err:?}");
    }

    #[test]
    fn invalid_peer_id_inside_scoped_entry_is_a_config_error() {
        let temp = tempfile::tempdir().expect("tempdir");
        write_allowlist(
            temp.path(),
            &serde_json::json!([{ "peer_id": "not-a-peer-id", "world_scope": ["world-a"] }]),
        );

        let err = load(temp.path(), &[]).expect_err("invalid scoped peer id rejected");
        assert!(matches!(err, CliError::Config(_)), "got {err:?}");
    }

    /// The `--allow-peer` overlay carries no scope by design; it must fill
    /// gaps only and never strip a hand-authored file scope.
    #[test]
    fn cli_overlay_never_strips_file_scope() {
        let temp = tempfile::tempdir().expect("tempdir");
        let peer = peer_id(9);
        write_allowlist(
            temp.path(),
            &serde_json::json!([{
                "peer_id": peer.to_string(),
                "world_scope": ["world-a"],
                "op_scope": ["upsert"],
            }]),
        );

        let scope = load(temp.path(), &[peer.to_string()]).expect("load with overlay");
        assert_eq!(scope.peer_ids(), vec![peer]);
        assert!(
            scope.allows_world(&peer, "world-a"),
            "CLI overlay must not strip a hand-authored file scope"
        );
    }

    /// A peer that is not allowlisted at all has no access (gate is
    /// fail-closed on unknown peers too).
    #[test]
    fn non_allowlisted_peer_has_no_access() {
        let temp = tempfile::tempdir().expect("tempdir");
        let listed = peer_id(10);
        write_allowlist(
            temp.path(),
            &serde_json::json!([{
                "peer_id": listed.to_string(),
                "world_scope": ["world-a"],
                "op_scope": ["upsert"],
            }]),
        );

        let scope = load(temp.path(), &[]).expect("load");
        let outsider = peer_id(11);
        assert_eq!(scope.access_for(&outsider), None);
        assert!(!scope.allows_world(&outsider, "world-a"));
        assert!(!scope.allows_op(&outsider, "upsert"));
        assert!(
            !scope.allows_module(&outsider, "basic-combat"),
            "a non-allowlisted peer has no module access"
        );
        assert!(
            scope.grant_for(&outsider).is_none(),
            "a non-allowlisted peer has no grant"
        );
    }

    // ---- v1.191 P1 T14: operator-stored Actor grants (durable §9) ----

    /// A well-formed grant round-trips: it is stored, bound to its own peer,
    /// and exposes the granted World viewpoint.
    #[test]
    fn v1191_holder_connect_grant_is_peer_bound() {
        let temp = tempfile::tempdir().expect("tempdir");
        let peer = peer_id(20);
        write_allowlist(
            temp.path(),
            &serde_json::json!([{
                "peer_id": peer.to_string(),
                "world_scope": ["world-a"],
                "op_scope": ["upsert"],
                "grant": {
                    "creator_id": "ctr_owner",
                    "actor": { "kind": "creator", "id": "ctr_owner" },
                    "world_id": "world-a",
                },
            }]),
        );

        let scope = load(temp.path(), &[]).expect("granted entry loads");
        let grant = scope.grant_for(&peer).expect("grant resolves for its peer");
        assert_eq!(grant.creator_id, "ctr_owner");
        assert_eq!(grant.world_id(), "world-a");
        assert_eq!(grant.actor_id(), "ctr_owner");
        assert_eq!(grant.binding_id, None);
        assert!(
            scope.grant_for(&peer_id(21)).is_none(),
            "another peer never inherits a stored grant"
        );
    }

    /// Absent grant ⇒ the peer keeps its scopes but no KE authority: the
    /// model never fabricates one from the peer id or the scopes.
    #[test]
    fn v1191_holder_connect_absent_grant_is_denied() {
        let temp = tempfile::tempdir().expect("tempdir");
        let peer = peer_id(22);
        write_allowlist(
            temp.path(),
            &serde_json::json!([{
                "peer_id": peer.to_string(),
                "world_scope": ["world-a"],
                "op_scope": ["upsert", "check", "assemble"],
            }]),
        );

        let scope = load(temp.path(), &[]).expect("grantless entry loads");
        assert!(scope.grant_for(&peer).is_none(), "no grant ⇒ no KE authority");
        assert!(
            scope.allows_op(&peer, "upsert"),
            "the op gate stays independent of the grant"
        );
    }

    /// A grant never widens the independent scopes; empty scopes deny even
    /// with a stored grant (fail-closed intersection).
    #[test]
    fn v1191_holder_connect_grant_does_not_widen_scopes() {
        let temp = tempfile::tempdir().expect("tempdir");
        let peer = peer_id(23);
        write_allowlist(
            temp.path(),
            &serde_json::json!([{
                "peer_id": peer.to_string(),
                "grant": {
                    "creator_id": "ctr_owner",
                    "actor": { "kind": "creator", "id": "ctr_owner" },
                    "world_id": "world-a",
                },
            }]),
        );

        let scope = load(temp.path(), &[]).expect("grant-only entry loads");
        assert!(scope.grant_for(&peer).is_some());
        assert!(!scope.allows_op(&peer, "upsert"), "no op_scope ⇒ denied");
        assert!(!scope.allows_world(&peer, "world-a"), "no world_scope ⇒ denied");
    }

    /// A Character grant requires its binding (the Character viewpoint is
    /// World + Character + binding) — an operator typo is a hard config
    /// error, never a grant that silently denies every invoke.
    #[test]
    fn v1191_holder_connect_character_grant_requires_a_binding() {
        let temp = tempfile::tempdir().expect("tempdir");
        write_allowlist(
            temp.path(),
            &serde_json::json!([{
                "peer_id": peer_id(24).to_string(),
                "grant": {
                    "creator_id": "ctr_owner",
                    "actor": { "kind": "character", "id": "chr_hero" },
                    "world_id": "world-a",
                },
            }]),
        );

        let err = load(temp.path(), &[]).expect_err("binding-less Character grant rejected");
        assert!(matches!(err, CliError::Config(_)), "got {err:?}");

        // With the binding it loads.
        write_allowlist(
            temp.path(),
            &serde_json::json!([{
                "peer_id": peer_id(24).to_string(),
                "grant": {
                    "creator_id": "ctr_owner",
                    "actor": { "kind": "character", "id": "chr_hero" },
                    "world_id": "world-a",
                    "binding_id": "bnd_hero",
                },
            }]),
        );
        let scope = load(temp.path(), &[]).expect("bound Character grant loads");
        let grant = scope.grant_for(&peer_id(24)).expect("grant present");
        assert_eq!(grant.actor_id(), "chr_hero");
        assert_eq!(grant.binding_id.as_deref(), Some("bnd_hero"));
    }

    /// A Creator grant's ActorRef is its controlling Creator — two different
    /// subjects are refused at load (never a silently dead grant).
    #[test]
    fn v1191_holder_connect_creator_grant_must_name_its_controlling_creator() {
        let temp = tempfile::tempdir().expect("tempdir");
        write_allowlist(
            temp.path(),
            &serde_json::json!([{
                "peer_id": peer_id(25).to_string(),
                "grant": {
                    "creator_id": "ctr_owner",
                    "actor": { "kind": "creator", "id": "ctr_someone_else" },
                    "world_id": "world-a",
                },
            }]),
        );

        let err = load(temp.path(), &[]).expect_err("mismatched Creator grant rejected");
        assert!(matches!(err, CliError::Config(_)), "got {err:?}");
    }

    /// Empty grant fields are refused at load (fail-closed, named error).
    #[test]
    fn v1191_holder_connect_empty_grant_fields_are_rejected() {
        for grant in [
            serde_json::json!({
                "creator_id": "",
                "actor": { "kind": "creator", "id": "" },
                "world_id": "world-a",
            }),
            serde_json::json!({
                "creator_id": "ctr_owner",
                "actor": { "kind": "creator", "id": "ctr_owner" },
                "world_id": "",
            }),
            serde_json::json!({
                "creator_id": "ctr_owner",
                "actor": { "kind": "character", "id": "chr_hero" },
                "world_id": "world-a",
                "binding_id": "",
            }),
        ] {
            let temp = tempfile::tempdir().expect("tempdir");
            write_allowlist(
                temp.path(),
                &serde_json::json!([{ "peer_id": peer_id(26).to_string(), "grant": grant }]),
            );
            let err = load(temp.path(), &[]).expect_err("empty grant field rejected");
            assert!(matches!(err, CliError::Config(_)), "got {err:?}");
        }
    }

    /// Unknown grant fields / an unknown actor kind stay hard config errors
    /// (`deny_unknown_fields` inside the grant, like the entry itself).
    #[test]
    fn v1191_holder_connect_unknown_grant_fields_are_rejected() {
        for grant in [
            serde_json::json!({
                "creator_id": "ctr_owner",
                "actor": { "kind": "creator", "id": "ctr_owner" },
                "world_id": "world-a",
                "world_scop": ["world-a"],
            }),
            serde_json::json!({
                "creator_id": "ctr_owner",
                "actor": { "kind": "deity", "id": "ctr_owner" },
                "world_id": "world-a",
            }),
        ] {
            let temp = tempfile::tempdir().expect("tempdir");
            write_allowlist(
                temp.path(),
                &serde_json::json!([{ "peer_id": peer_id(27).to_string(), "grant": grant }]),
            );
            let err = load(temp.path(), &[]).expect_err("unknown grant field rejected");
            assert!(matches!(err, CliError::Config(_)), "got {err:?}");
        }
    }

    /// The `--allow-peer` overlay can never mint or strip a grant (it has no
    /// scope fields by design — the trust root stays hand-authored).
    #[test]
    fn v1191_holder_connect_cli_overlay_never_mints_a_grant() {
        let temp = tempfile::tempdir().expect("tempdir");
        let overlay = peer_id(28);
        let scope = load(temp.path(), &[overlay.to_string()]).expect("overlay loads");
        assert!(
            scope.grant_for(&overlay).is_none(),
            "a CLI overlay entry carries no grant (fail-closed)"
        );
    }

    /// The granted World viewpoint is independent from `world_scope`: a
    /// grant on world-a never authorizes world-b, and a scope entry for
    /// world-b never re-points the grant.
    #[test]
    fn v1191_holder_connect_grant_viewpoint_is_not_rewritten_by_scope() {
        let temp = tempfile::tempdir().expect("tempdir");
        let peer = peer_id(29);
        write_allowlist(
            temp.path(),
            &serde_json::json!([{
                "peer_id": peer.to_string(),
                "world_scope": ["world-a", "world-b"],
                "op_scope": ["upsert"],
                "grant": {
                    "creator_id": "ctr_owner",
                    "actor": { "kind": "creator", "id": "ctr_owner" },
                    "world_id": "world-a",
                },
            }]),
        );

        let scope = load(temp.path(), &[]).expect("multi-world entry loads");
        let grant = scope.grant_for(&peer).expect("grant present");
        assert_eq!(grant.world_id(), "world-a", "the grant viewpoint is stored, not derived");
        assert!(
            scope.allows_world(&peer, "world-b"),
            "the scope gate stays independent (the grant is a second, narrowing gate)"
        );
    }
}
