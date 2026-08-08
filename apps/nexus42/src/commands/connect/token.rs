//! Capability-token issuance (`nexus42 connect token issue`) — V1.155 P1
//! production issuance (iteration spec Design #1).
//!
//! Issuer key lifecycle: `~/.nexus42/connect/issuer.key` (Ed25519, libp2p
//! protobuf encoding, create-once 0600 — architect lock #4: a DISTINCT file
//! from `identity.key`; the node identity and the token issuer are
//! different trust roles). The issuer `peer_id` derives from the key
//! (spoke `derive_peer_id_from_ed25519_pubkey`) and MUST equal
//! `claims.iss` (spoke normative rule).
//!
//! The CLI prints the signed wire proof `{v, claims, sig}` as JSON on
//! stdout — no secrets echoed.

use crate::errors::{CliError, Result};
use clap::Subcommand;
use libp2p::identity::Keypair;
use spoke_connect::core::{
    derive_peer_id_from_ed25519_pubkey, issue_capability_token, CapabilityClaims,
    CapabilityTokenProof,
};
use spoke_connect::CapabilityTokenProvider;
use std::path::Path;
use std::sync::Arc;

/// `connect token` subcommands.
#[derive(Debug, Subcommand)]
pub enum TokenCommand {
    /// Issue a signed capability token with the persisted issuer key.
    ///
    /// Prints the wire proof `{v, claims, sig}` (JCS claims, base64url
    /// signature) as JSON on stdout.
    Issue {
        /// Subject peer id — who may present the token.
        #[arg(long, value_name = "PEER_ID")]
        sub: String,
        /// Audience peer id — the verifying node.
        #[arg(long, value_name = "PEER_ID")]
        aud: String,
        /// Comma-separated capability names granted to `sub` (non-empty).
        #[arg(long, value_name = "C1,C2")]
        capabilities: String,
        /// Expiry as Unix time seconds (UTC); must be beyond
        /// now + 60s clock skew.
        #[arg(long, value_name = "UNIX_SECONDS")]
        exp: u64,
        /// Issuer peer id override (defaults to the issuer key's derived
        /// peer id — MUST equal it, spoke normative rule).
        #[arg(long, value_name = "PEER_ID")]
        iss: Option<String>,
    },
}

/// Load the persisted capability-token issuer key, or generate + persist a
/// fresh Ed25519 keypair on first use (create-once, 0600 on Unix).
///
/// Mirrors [`super::identity::load_or_create_identity`]: same libp2p
/// protobuf file format, same atomic create-with-mode (0600), same
/// self-healing reload hardening, same corrupt-file rejection. The file is
/// DISTINCT from `identity.key` (V1.155 P1 architect lock #4): the node
/// identity and the token issuer are different trust roles.
///
/// # Parameters
/// `home` is the **raw user home** (`$HOME`); this fn joins `.nexus42`
/// internally via `nexus_home_layout::connect_issuer_key_path`, so callers
/// MUST NOT pre-join `~/.nexus42`.
///
/// # Errors
/// Returns [`CliError::Io`] on filesystem failure, or [`CliError::Config`]
/// when an existing key file is corrupt or unreadable.
pub fn load_or_create_issuer_key(home: &Path) -> Result<Keypair> {
    use std::io::Write;

    let path = nexus_home_layout::connect_issuer_key_path(home);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Serialize before touching the filesystem so a serialization failure
    // can never leave a partial key file behind.
    let keypair = Keypair::generate_ed25519();
    let encoded = keypair
        .to_protobuf_encoding()
        .map_err(|e| CliError::Config(format!("issuer key serialization failed: {e}")))?;

    // `create_new` eliminates the TOCTOU race between the existence check
    // and the write (same pattern as the identity key): create-once, never
    // overwrite — a second issue reuses the persisted key.
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    // W-1: apply the owner-only mode at creation time (atomic create-with-
    // mode) so the key is never observable at a permissive mode — not even
    // if the process crashes between open and write. umask can only tighten
    // the bits, never loosen them. On non-unix, the platform default
    // applies (best available).
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    match options.open(&path) {
        Ok(mut file) => {
            if let Err(e) = file.write_all(&encoded) {
                // S-1: never leave a partial key file behind — a corrupt key
                // would block every later issue (the reload path rejects it).
                let _ = std::fs::remove_file(&path);
                return Err(CliError::Io(e));
            }
            Ok(keypair)
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => load_issuer_key_at(&path),
        Err(e) => Err(CliError::Io(e)),
    }
}

/// Ensure the issuer key file is owner-only (0600) on the reload path.
///
/// Same hardening rationale as the identity key: files created before the
/// atomic `mode(0o600)` fix may sit at a permissive mode; self-heal instead
/// of refusing to issue.
#[cfg(unix)]
fn harden_issuer_key_permissions(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let mode = std::fs::metadata(path)?.permissions().mode();
    if (mode & 0o777) > 0o600 {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

/// Derive the issuer `peer_id` for a keypair (spoke normative mapping:
/// identity multihash over the Ed25519 protobuf public key).
///
/// # Errors
/// [`CliError::Config`] when the keypair is not Ed25519.
pub fn issuer_peer_id(keypair: &Keypair) -> Result<String> {
    let public = keypair
        .public()
        .try_into_ed25519()
        .map_err(|_| CliError::Config("issuer key is not an Ed25519 keypair".into()))?;
    Ok(derive_peer_id_from_ed25519_pubkey(&public.to_bytes()))
}

/// Extract the raw 32-byte Ed25519 seed the spoke issue API signs with.
///
/// # Errors
/// [`CliError::Config`] when the keypair is not Ed25519 or the secret is
/// not exactly 32 bytes.
pub(crate) fn issuer_secret_bytes(keypair: &Keypair) -> Result<[u8; 32]> {
    let ed25519 = keypair
        .clone()
        .try_into_ed25519()
        .map_err(|_| CliError::Config("issuer key is not an Ed25519 keypair".into()))?;
    <[u8; 32]>::try_from(ed25519.secret().as_ref())
        .map_err(|_| CliError::Config("issuer key is not a 32-byte Ed25519 secret".into()))
}

/// Load the persisted issuer key from an explicit path (load-only).
///
/// The CLI (`connect token issue`) is the key-creation path (architect
/// lock #4); an enabled provider requires the key to already exist — a
/// missing key is a fail-closed boot error, never a silent create.
///
/// # Errors
/// [`CliError::Config`] when the file is missing or not a valid Ed25519
/// key, or [`CliError::Io`] on filesystem failure.
pub(crate) fn load_issuer_key_at(path: &Path) -> Result<Keypair> {
    let bytes = std::fs::read(path).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => CliError::Config(format!(
            "issuer key not found at {}: run `nexus42 connect token issue` once \
             to create it (the CLI is the issuer-key creation path)",
            path.display()
        )),
        _ => CliError::Io(e),
    })?;
    #[cfg(unix)]
    harden_issuer_key_permissions(path)?;
    Keypair::from_protobuf_encoding(&bytes).map_err(|e| {
        CliError::Config(format!("invalid Connect issuer key at {}: {e}", path.display()))
    })
}

/// TTL for provider-minted proofs (seconds, V1.155 P1): proofs are minted
/// on demand at challenge time; 5 minutes covers handshake exchanges
/// without keeping long-lived tokens around.
///
/// Clock-skew interplay (QC2 F-002): the ±60s skew guard is applied at
/// *issuance* (spoke rejects `exp <= now + 60`), and the proof is verified
/// exactly once at challenge-response — the session stores a boolean grant
/// afterwards and never re-checks `exp` mid-session. The 300s lifetime
/// therefore only needs to outlive the one-shot challenge round-trip and
/// clear the 60s guard: a verifier clock up to ~4 min ahead of the minting
/// host still accepts, and beyond that the dial fails loudly (fail-closed,
/// recoverable — never a silent grant).
pub const PROVIDER_TOKEN_TTL_SECONDS: u64 = 300;

/// Build the mint-on-demand [`CapabilityTokenProvider`] closure for an
/// enabled provider (iteration spec Design #2).
///
/// The closure loads nothing itself — the issuer keypair is loaded at boot
/// (`load_issuer_key_at`) — and mints a fresh proof for every challenge:
/// `iss` = the issuer-derived peer id, `sub` = this node's peer id,
/// `aud` = the challenger, `capabilities` = this host's manifest
/// capabilities (what the node actually serves — the token can never grant
/// more than the host advertises), `exp` = now + [`PROVIDER_TOKEN_TTL_SECONDS`].
///
/// Execution contract: the closure runs synchronously on the node's
/// network event loop (spoke [`CapabilityTokenProvider`] docs) — no I/O:
/// all key material is captured at boot.
///
/// # Errors
/// [`CliError::Config`] when the issuer keypair is not Ed25519 or its
/// secret is not exactly 32 bytes.
pub(crate) fn build_provider(
    issuer: &Keypair,
    sub: String,
    capabilities: Vec<String>,
) -> Result<Arc<CapabilityTokenProvider>> {
    let iss = issuer_peer_id(issuer)?;
    let seed = issuer_secret_bytes(issuer)?;
    let provider = move |aud: &str| -> std::result::Result<serde_json::Value, String> {
        let now = now_unix_seconds()?;
        let claims = CapabilityClaims {
            iss: iss.clone(),
            sub: sub.clone(),
            aud: aud.to_string(),
            capabilities: capabilities.clone(),
            exp: now + PROVIDER_TOKEN_TTL_SECONDS,
            iat: Some(now),
            jti: None,
        };
        let proof = issue_capability_token(&seed, claims, now)
            .map_err(|e| format!("capability-token issuance failed: {e}"))?;
        serde_json::to_value(&proof).map_err(|e| format!("proof serialization failed: {e}"))
    };
    Ok(Arc::new(provider))
}

/// Issue a signed capability token with the persisted issuer key.
///
/// `iss` defaults to the issuer-derived peer id and MUST equal it (spoke
/// normative rule: "the issuer key MUST derive claims.iss"); spoke
/// `issue_capability_token` enforces the same rule on the bytes. `iat` is
/// stamped at `now` (within the ±60s clock-skew window by construction).
///
/// # Errors
/// [`CliError::Config`] on empty/blank `sub`/`aud` (QC2 F-001: clap
/// enforces *presence* only — an empty value would mint an inert token,
/// since spoke binds `sub` to the noise-authenticated session peer and
/// `aud` to the challenger, both always non-empty peer ids), `--iss`
/// mismatch, key-file corruption, or spoke issuance rejection
/// (non-empty capabilities, `exp` beyond `now + 60s` skew,
/// issuer/claims mismatch).
pub fn issue_token(
    home: &Path,
    sub: &str,
    aud: &str,
    capabilities: &[String],
    exp: u64,
    iss: Option<&str>,
    now: u64,
) -> Result<CapabilityTokenProof> {
    // Validated BEFORE the issuer-key load so a usage error has no
    // filesystem side effect (the key file is created only on a valid
    // issue, not on flag misuse).
    if sub.trim().is_empty() {
        return Err(CliError::Config(
            "--sub must be a non-empty peer id (the subject who may present the token)".into(),
        ));
    }
    if aud.trim().is_empty() {
        return Err(CliError::Config(
            "--aud must be a non-empty peer id (the verifying node)".into(),
        ));
    }
    let keypair = load_or_create_issuer_key(home)?;
    let derived_iss = issuer_peer_id(&keypair)?;
    let iss = iss.unwrap_or(&derived_iss);
    if iss != derived_iss.as_str() {
        return Err(CliError::Config(format!(
            "issuer key derives peer id {derived_iss}, not --iss {iss}: \
             the issuer key MUST derive claims.iss (spoke normative rule)"
        )));
    }
    let claims = CapabilityClaims {
        iss: iss.to_string(),
        sub: sub.to_string(),
        aud: aud.to_string(),
        capabilities: capabilities.to_vec(),
        exp,
        iat: Some(now),
        jti: None,
    };
    let seed = issuer_secret_bytes(&keypair)?;
    issue_capability_token(&seed, claims, now)
        .map_err(|e| CliError::Config(format!("token issuance rejected: {e}")))
}

/// Parse the `--capabilities` flag (`c1,c2`): non-empty list, no empty
/// entries (an empty entry is ambiguous — usage error).
///
/// # Errors
/// [`CliError::Config`] when the list is empty or contains an empty entry.
fn parse_capabilities(raw: &str) -> Result<Vec<String>> {
    if raw.trim().is_empty() {
        return Err(CliError::Config(
            "--capabilities must be a non-empty comma-separated list \
             (e.g. --capabilities spoke-baseline,l2-computable)"
                .into(),
        ));
    }
    let mut capabilities = Vec::new();
    for item in raw.split(',') {
        let item = item.trim();
        if item.is_empty() {
            return Err(CliError::Config(
                "--capabilities contains an empty entry (e.g. `a,,b`): \
                 each capability must be a non-empty name"
                    .into(),
            ));
        }
        capabilities.push(item.to_string());
    }
    Ok(capabilities)
}

/// Current time in Unix seconds (the unit spoke `issue_capability_token`
/// and `verify_capability_token` operate on).
///
/// # Errors
/// `Err` when the system clock is before the Unix epoch (pre-epoch clock
/// failure). QC3-001: the failure must surface as a mint error instead of
/// silently minting a degenerate proof (`iat=0` / `exp=300`) that every
/// healthy verifier would reject — a loud provider error beats a channel
/// drop that looks like a generic provider failure.
fn now_unix_seconds() -> std::result::Result<u64, String> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .map_err(|e| format!("system clock is before the Unix epoch; refusing to mint: {e}"))
}

/// Run a `connect token` command.
///
/// # Errors
/// [`CliError::Config`] on flag-validation failures (empty/blank
/// `--sub`/`--aud`, empty capabilities, `--iss` ≠ derived issuer, exp
/// within the clock-skew window), key-file corruption, or spoke issuance
/// rejection.
pub fn run(command: TokenCommand) -> Result<()> {
    match command {
        TokenCommand::Issue {
            sub,
            aud,
            capabilities,
            exp,
            iss,
        } => {
            let home = crate::config::user_home_dir()?;
            let capabilities = parse_capabilities(&capabilities)?;
            let proof = issue_token(
                &home,
                &sub,
                &aud,
                &capabilities,
                exp,
                iss.as_deref(),
                now_unix_seconds().map_err(CliError::Other)?,
            )?;
            let json = serde_json::to_string(&proof).map_err(CliError::Json)?;
            println!("{json}");
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use spoke_connect::core::{verify_capability_token, CapabilityTokenProof};

    /// Fixed "now" (Unix seconds) for deterministic issuance tests.
    const NOW: u64 = 1_750_000_000;

    fn temp_home() -> tempfile::TempDir {
        tempfile::tempdir().expect("tempdir")
    }

    fn issue_ok(
        home: &Path,
        sub: &str,
        aud: &str,
        caps: &[&str],
        exp: u64,
    ) -> CapabilityTokenProof {
        let caps: Vec<String> = caps.iter().map(std::string::ToString::to_string).collect();
        issue_token(home, sub, aud, &caps, exp, None, NOW).expect("issue succeeds")
    }

    fn issuer_of(home: &Path) -> String {
        let keypair = load_or_create_issuer_key(home).expect("reload key");
        issuer_peer_id(&keypair).expect("derive issuer")
    }

    #[test]
    fn issued_token_verifies_green_with_derived_issuer() {
        let home = temp_home();
        let sub = "subject-peer";
        let aud = "audience-peer";
        let proof = issue_ok(home.path(), sub, aud, &["spoke-baseline"], NOW + 3600);

        assert_eq!(proof.v, spoke_connect::core::TOKEN_VERSION);
        assert_eq!(proof.claims.sub, sub);
        assert_eq!(proof.claims.aud, aud);
        assert_eq!(proof.claims.capabilities, vec!["spoke-baseline".to_string()]);
        assert!(!proof.sig.is_empty(), "signature must be present");

        // Default iss = the issuer key's derived peer id.
        let issuer = issuer_of(home.path());
        assert_eq!(proof.claims.iss, issuer, "default iss = issuer-derived peer id");

        // Issue → verify green with the correct trusted_issuers.
        let granted = verify_capability_token(&proof, &[issuer], aud, sub, NOW + 10)
            .expect("token verifies green");
        assert_eq!(granted, vec!["spoke-baseline".to_string()]);
    }

    #[test]
    fn issued_token_stamps_iat_at_issuance() {
        let home = temp_home();
        let proof = issue_ok(home.path(), "subject-peer", "audience-peer", &["spoke-baseline"], NOW + 3600);
        assert_eq!(proof.claims.iat, Some(NOW), "iat = issuance time");
    }

    #[test]
    fn iss_override_matching_derived_id_is_accepted() {
        let home = temp_home();
        let issuer = issuer_of(home.path());
        let caps = vec!["spoke-baseline".to_string()];
        let proof = issue_token(
            home.path(),
            "subject-peer",
            "audience-peer",
            &caps,
            NOW + 3600,
            Some(&issuer),
            NOW,
        )
        .expect("explicit iss == derived issuer is accepted");
        assert_eq!(proof.claims.iss, issuer);
    }

    #[test]
    fn iss_override_mismatch_rejected_at_issue() {
        let home = temp_home();
        let other = libp2p::identity::Keypair::generate_ed25519();
        let other_iss = other.public().to_peer_id().to_string();
        let caps = vec!["spoke-baseline".to_string()];
        let err = issue_token(
            home.path(),
            "subject-peer",
            "audience-peer",
            &caps,
            NOW + 3600,
            Some(&other_iss),
            NOW,
        )
        .expect_err("--iss override that != derived peer id must be rejected");
        assert!(
            matches!(err, CliError::Config(_)),
            "iss mismatch is a configuration error: {err:?}"
        );
    }

    #[test]
    fn tampered_claims_rejected() {
        let home = temp_home();
        let sub = "subject-peer";
        let aud = "audience-peer";
        let mut proof = issue_ok(home.path(), sub, aud, &["spoke-baseline"], NOW + 3600);
        // Tamper after signing: grant a second capability.
        proof.claims.capabilities.push("l2-computable".to_string());

        let err = verify_capability_token(&proof, &[issuer_of(home.path())], aud, sub, NOW + 10)
            .expect_err("tampered claims must fail verification");
        assert!(
            matches!(err, spoke_connect::core::CoreError::TokenInvalid(_)),
            "tampered claims rejected: {err:?}"
        );
    }

    #[test]
    fn tampered_signature_rejected() {
        let home = temp_home();
        let sub = "subject-peer";
        let aud = "audience-peer";
        let mut proof = issue_ok(home.path(), sub, aud, &["spoke-baseline"], NOW + 3600);
        let mut sig: Vec<char> = proof.sig.chars().collect();
        sig[0] = if sig[0] == 'A' { 'B' } else { 'A' };
        proof.sig = sig.into_iter().collect();

        let err = verify_capability_token(&proof, &[issuer_of(home.path())], aud, sub, NOW + 10)
            .expect_err("tampered signature must fail verification");
        assert!(
            matches!(err, spoke_connect::core::CoreError::TokenInvalid(_)),
            "tampered signature rejected: {err:?}"
        );
    }

    #[test]
    fn expired_token_rejected() {
        let home = temp_home();
        let sub = "subject-peer";
        let aud = "audience-peer";
        // exp = NOW + 61: valid at issuance (> NOW + 60s skew), expired
        // once the verifier's now reaches it (reject when now >= exp).
        let proof = issue_ok(home.path(), sub, aud, &["spoke-baseline"], NOW + 61);

        let err = verify_capability_token(
            &proof,
            &[issuer_of(home.path())],
            aud,
            sub,
            NOW + 61,
        )
        .expect_err("expired token must be rejected");
        assert!(
            matches!(err, spoke_connect::core::CoreError::TokenInvalid(_)),
            "expired token rejected: {err:?}"
        );
    }

    #[test]
    fn wrong_issuer_rejected() {
        let home = temp_home();
        let sub = "subject-peer";
        let aud = "audience-peer";
        let proof = issue_ok(home.path(), sub, aud, &["spoke-baseline"], NOW + 3600);

        // A different trusted issuer: a fresh keypair's peer id.
        let other = libp2p::identity::Keypair::generate_ed25519();
        let other_iss = other.public().to_peer_id().to_string();
        let err = verify_capability_token(&proof, &[other_iss], aud, sub, NOW + 10)
            .expect_err("token from an untrusted issuer must be rejected");
        assert!(
            matches!(err, spoke_connect::core::CoreError::TokenInvalid(_)),
            "wrong-issuer token rejected: {err:?}"
        );
    }

    #[test]
    fn exp_within_clock_skew_rejected_at_issue() {
        // Spoke fail-fast parity: exp must be beyond now + 60s, else the
        // verifier would deterministically reject.
        let home = temp_home();
        let caps = vec!["spoke-baseline".to_string()];
        let err = issue_token(
            home.path(),
            "subject-peer",
            "audience-peer",
            &caps,
            NOW + 60,
            None,
            NOW,
        )
        .expect_err("exp within the clock-skew window must be rejected at issuance");
        assert!(
            matches!(err, CliError::Config(_)),
            "exp-in-skew is a configuration error: {err:?}"
        );
    }

    #[test]
    fn issuer_key_created_once_and_reused() {
        let home = temp_home();
        let created_issuer = issuer_of(home.path());
        let reloaded_issuer = issuer_of(home.path());
        assert_eq!(
            reloaded_issuer, created_issuer,
            "issuer key must persist across calls"
        );

        // create-once: reload must never overwrite the file bytes.
        let path = nexus_home_layout::connect_issuer_key_path(home.path());
        let bytes1 = std::fs::read(&path).expect("read key");
        let _ = load_or_create_issuer_key(home.path()).expect("reload again");
        let bytes2 = std::fs::read(&path).expect("read key");
        assert_eq!(bytes1, bytes2, "create-once: reload must not overwrite");
    }

    #[cfg(unix)]
    #[test]
    fn issuer_key_file_is_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let home = temp_home();
        let _ = load_or_create_issuer_key(home.path()).expect("create");
        let path = nexus_home_layout::connect_issuer_key_path(home.path());
        let mode = std::fs::metadata(&path)
            .expect("metadata")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600, "issuer key must be owner-only (0600)");
    }

    #[test]
    fn issuer_key_is_distinct_from_identity_key() {
        // Architect lock #4: `issuer.key` is a DISTINCT file from
        // `identity.key` — different trust roles, different keypairs.
        let home = temp_home();
        let _ = load_or_create_issuer_key(home.path()).expect("create issuer");
        let _ = super::super::identity::load_or_create_identity(home.path()).expect("create identity");

        let issuer_path = nexus_home_layout::connect_issuer_key_path(home.path());
        let identity_path = nexus_home_layout::connect_identity_key_path(home.path());
        assert_ne!(issuer_path, identity_path, "distinct file paths");

        let issuer_peer = issuer_of(home.path());
        let identity_key = super::super::identity::load_or_create_identity(home.path()).expect("reload identity");
        let identity_peer = issuer_peer_id(&identity_key).expect("derive identity peer");
        assert_ne!(
            issuer_peer, identity_peer,
            "issuer and identity keys must be distinct keypairs"
        );
    }

    #[test]
    fn corrupt_issuer_key_rejected() {
        let home = temp_home();
        let path = nexus_home_layout::connect_issuer_key_path(home.path());
        std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        std::fs::write(&path, b"not-a-keypair").expect("write");

        let err = load_or_create_issuer_key(home.path()).expect_err("corrupt key must be rejected");
        assert!(
            matches!(err, CliError::Config(_)),
            "corrupt issuer key is a configuration error: {err:?}"
        );
    }

    #[test]
    fn capabilities_require_non_empty_list() {
        assert!(
            matches!(parse_capabilities(""), Err(CliError::Config(_))),
            "empty list rejected"
        );
        assert!(
            matches!(parse_capabilities("   "), Err(CliError::Config(_))),
            "blank list rejected"
        );
    }

    #[test]
    fn empty_sub_rejected() {
        // QC2 F-001: clap enforces presence only — an empty `--sub ""`
        // must be a usage error, not an inert token.
        let home = temp_home();
        let caps = vec!["spoke-baseline".to_string()];
        for bad in ["", "   ", "\t"] {
            let err = issue_token(
                home.path(),
                bad,
                "audience-peer",
                &caps,
                NOW + 3600,
                None,
                NOW,
            )
            .expect_err("empty/blank sub must be rejected");
            assert!(
                matches!(err, CliError::Config(_)),
                "empty sub is a usage error: {err:?}"
            );
        }
        // A usage error must not create the issuer key file (no side effect).
        let path = nexus_home_layout::connect_issuer_key_path(home.path());
        assert!(
            !path.exists(),
            "rejected issue must not create the issuer key"
        );
    }

    #[test]
    fn empty_aud_rejected() {
        // QC2 F-001: same for `--aud`.
        let home = temp_home();
        let caps = vec!["spoke-baseline".to_string()];
        for bad in ["", "   "] {
            let err = issue_token(
                home.path(),
                "subject-peer",
                bad,
                &caps,
                NOW + 3600,
                None,
                NOW,
            )
            .expect_err("empty/blank aud must be rejected");
            assert!(
                matches!(err, CliError::Config(_)),
                "empty aud is a usage error: {err:?}"
            );
        }
        let path = nexus_home_layout::connect_issuer_key_path(home.path());
        assert!(
            !path.exists(),
            "rejected issue must not create the issuer key"
        );
    }

    #[test]
    fn token_from_untrusted_issuer_rejected_through_verify_path() {
        // QC2 F-003: a token minted by an issuer NOT in `trusted_issuers`
        // must be rejected through the verify path. Mint with a second
        // (untrusted) issuer key and verify against the trusted issuer's
        // whitelist — a regressed issuer-whitelist wiring would fail this.
        let trusted_home = temp_home();
        let trusted_iss = issuer_of(trusted_home.path());

        let untrusted_home = temp_home();
        let proof = issue_ok(
            untrusted_home.path(),
            "subject-peer",
            "audience-peer",
            &["spoke-baseline"],
            NOW + 3600,
        );
        let untrusted_iss = issuer_of(untrusted_home.path());
        assert_ne!(
            untrusted_iss, trusted_iss,
            "a second home must mint with a distinct issuer key"
        );

        let err = verify_capability_token(
            &proof,
            &[trusted_iss],
            "audience-peer",
            "subject-peer",
            NOW + 10,
        )
        .expect_err("token minted by an untrusted issuer must be rejected");
        assert!(
            matches!(err, spoke_connect::core::CoreError::TokenInvalid(_)),
            "untrusted-issuer token rejected: {err:?}"
        );
    }

    #[test]
    fn capabilities_reject_empty_entries() {
        for raw in ["a,,b", "a,", ",a", ",", "spoke-baseline, l2-computable,"] {
            assert!(
                matches!(parse_capabilities(raw), Err(CliError::Config(_))),
                "ambiguous capabilities {raw:?} rejected"
            );
        }
    }

    #[test]
    fn capabilities_parse_comma_separated_list() {
        assert_eq!(
            parse_capabilities("spoke-baseline").expect("single"),
            vec!["spoke-baseline".to_string()]
        );
        assert_eq!(
            parse_capabilities("spoke-baseline, l2-computable").expect("pair"),
            vec!["spoke-baseline".to_string(), "l2-computable".to_string()]
        );
    }
}
