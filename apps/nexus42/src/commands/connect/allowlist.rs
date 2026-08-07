//! Connect Host peer allowlist (`~/.nexus42/connect/allowlist.json` +
//! repeatable `--allow-peer` overlay).
//!
//! N-C0 product contract (draft §2.3): the allowlist is the trust root.
//! N-C1 world scoping (P1 spec § World scoping — schema locked): each
//! `peer_ids` entry is either a bare `"12D3…"` peer id (N-C0 shape — no
//! write access) or an object `{ "peer_id": "12D3…", "world_scope":
//! ["<world-uuid>", …], "op_scope": ["upsert","promote","relate"] }`.
//! Both scopes are optional and **fail-closed**: an absent/empty scope
//! denies world writes — a bare entry (or a `--allow-peer` overlay) is
//! handshake-allowlisted but can never write. World ids are world UUID
//! strings, never filesystem paths. A missing file ⇒ empty list ⇒
//! **fail-closed** (spoke-connect rejects every remote peer). The operator
//! edits the allowlist out-of-band; there is no online enroll endpoint.

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

/// One `peer_ids` entry: a bare peer id (N-C0 shape — no write access) or a
/// scoped object (N-C1).
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum PeerEntry {
    Bare(String),
    Scoped(PeerEntryScoped),
}

/// Scoped entry form — locked schema (P1 spec § World scoping).
///
/// `world_scope` / `op_scope` are optional; absent fields deserialize to
/// empty lists and the gate then denies every world/op (fail-closed).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PeerEntryScoped {
    peer_id: String,
    #[serde(default)]
    world_scope: Vec<String>,
    #[serde(default)]
    op_scope: Vec<String>,
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
/// allowlisted but has no world-write access.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PeerAccess {
    /// World ids (world UUID strings, not paths) this peer may write to.
    pub world_scope: BTreeSet<String>,
    /// Write ops this peer may invoke (N-C1 served ops: `upsert` / `promote`
    /// / `relate`).
    pub op_scope: BTreeSet<String>,
}

/// The served write ops (N-C1) — the `op_scope` members that make a peer
/// write-capable for the multi-write-peer boot warning. Kept in lockstep
/// with `invoke::SERVED_OPS` (same wire strings); the interop honesty
/// machine-check pins the manifest side to that const, and this const only
/// feeds a conservative boot warning, so drift here cannot open a gate.
const WRITE_OPS: [&str; 3] = ["upsert", "promote", "relate"];

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

    /// True when no peer is allowlisted at all (missing/empty file).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Number of allowlisted peers holding any write scope: a peer counts
    /// when its `op_scope` contains any served write op ([`WRITE_OPS`]), or
    /// when it carries a non-empty `world_scope` (world scoping expresses
    /// write intent for those worlds). Over-approximates by design — this
    /// feeds a boot warning (see [`warn_multi_write_peer`]), and warning
    /// about a peer that turns out write-less is harmless, while NOT
    /// warning about a write-capable peer is the failure mode the warning
    /// exists for.
    #[must_use]
    pub fn write_capable_peer_count(&self) -> usize {
        self.entries
            .values()
            .filter(|access| {
                access
                    .op_scope
                    .iter()
                    .any(|op| WRITE_OPS.contains(&op.as_str()))
                    || !access.world_scope.is_empty()
            })
            .count()
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
                self.entries.insert(
                    peer,
                    PeerAccess {
                        world_scope: scoped.world_scope.into_iter().collect(),
                        op_scope: scoped.op_scope.into_iter().collect(),
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

/// Boot-time warning (plan QC, QC2 W-1 cheap hardening): the allowlist
/// holds more than one write-scoped peer.
///
/// The per-invoke caller peer id is payload-carried
/// (`extensions.nexus.peer_id`) and spoofable — the locked spoke-connect
/// 0.9.1 handler signature (`dyn Fn(&str, Value)`) carries no authenticated
/// session peer — so with more than one write-scoped allowlisted peer,
/// per-peer world/op scoping silently degrades to the union of all scopes
/// (any allowlisted peer can put another's id in the envelope and inherit
/// its full write scope).
///
/// N-C1 is accepted ONLY while the allowlist holds at most one write-
/// capable peer (spec §10.6). This is a **warning, not a refusal**: the
/// operator may have a legitimate reason (e.g. a CLI-overlay peer for
/// manual writes). The E2 fix is session-bound identity (upstream handler
/// signature change) or capability-token auth.
///
/// Writes to `sink` (boot callers pass `std::io::stderr()`; tests pass a
/// buffer).
pub fn warn_multi_write_peer(scope: &PeerScope, sink: &mut dyn std::io::Write) {
    let write_capable = scope.write_capable_peer_count();
    if write_capable > 1 {
        let _ = writeln!(
            sink,
            "nexus42 connect start: WARNING: {write_capable} allowlisted peers hold write \
             scope. The per-invoke caller peer_id is payload-carried (spoofable): any \
             allowlisted peer can impersonate another and inherit its full world/op scope, \
             so per-peer scoping degrades to the union of all scopes. N-C1 assumes at most \
             one write-capable peer — with more, split write peers into separate processes \
             or accept the risk deliberately."
        );
    }
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
    }

    // ---- Plan QC fix wave (QC2 W-1): multi-write-peer boot warning ----

    /// The N-C1 trust precondition (spec §10.6): the per-invoke caller
    /// `peer_id` is payload-carried (`extensions.nexus.peer_id`) and
    /// spoofable — the locked spoke-connect 0.9.1 handler signature carries
    /// no session peer — so per-peer world/op scoping is a real boundary
    /// ONLY while the allowlist holds at most one write-capable peer. More
    /// than one ⇒ any allowlisted peer can impersonate another and the
    /// scoping silently degrades to the union of all scopes. The boot
    /// warning must fire for a multi-write-peer allowlist and name the
    /// spoofing risk + the single-write-peer precondition.
    #[test]
    fn multi_write_peer_allowlist_emits_boot_warning() {
        let temp = tempfile::tempdir().expect("tempdir");
        let peer_a = peer_id(12);
        let peer_b = peer_id(13);
        write_allowlist(
            temp.path(),
            &serde_json::json!([
                {
                    "peer_id": peer_a.to_string(),
                    "world_scope": ["world-a"],
                    "op_scope": ["upsert"],
                },
                {
                    "peer_id": peer_b.to_string(),
                    "world_scope": ["world-a"],
                    "op_scope": ["relate"],
                },
            ]),
        );

        let scope = load(temp.path(), &[]).expect("multi-peer allowlist loads");
        assert_eq!(
            scope.write_capable_peer_count(),
            2,
            "both scoped peers hold write scope"
        );

        let mut sink = Vec::new();
        warn_multi_write_peer(&scope, &mut sink);
        let output = String::from_utf8(sink).expect("warning is utf8");
        assert!(
            output.contains("WARNING"),
            "multi-write-peer allowlist must warn at boot: {output:?}"
        );
        assert!(
            output.contains("spoof") && output.contains("at most one"),
            "warning must name the spoofing risk and the single-write-peer \
             precondition: {output:?}"
        );
    }

    /// The single-write-peer deployment (the N-C1 acceptance shape: one
    /// scoped write peer + bare allowlisted peers) must NOT warn.
    #[test]
    fn single_write_peer_allowlist_does_not_warn() {
        let temp = tempfile::tempdir().expect("tempdir");
        let peer = peer_id(14);
        let bare = peer_id(15);
        write_allowlist(
            temp.path(),
            &serde_json::json!([
                {
                    "peer_id": peer.to_string(),
                    "world_scope": ["world-a"],
                    "op_scope": ["upsert", "promote", "relate"],
                },
                bare.to_string(),
            ]),
        );

        let scope = load(temp.path(), &[]).expect("single-write-peer allowlist loads");
        assert_eq!(
            scope.write_capable_peer_count(),
            1,
            "bare entries carry no write scope"
        );

        let mut sink = Vec::new();
        warn_multi_write_peer(&scope, &mut sink);
        assert!(
            sink.is_empty(),
            "single write-capable peer must not warn: {:?}",
            String::from_utf8_lossy(&sink)
        );
    }
}
