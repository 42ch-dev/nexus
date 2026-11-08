# nexus-config-metadata — Policy Bridge for Atomic Publication

Private helper crate for `nexus-home-layout`'s whole-document `config.toml`
publisher. It captures the old document's access policy and applies it to the
staged replacement before the atomic rename, so a replacement keeps the owner,
group, mode, ACL and extended access attributes it had.

## Key Rules

- **This crate is the workspace's only `unsafe` exception.** It does not inherit
  the workspace lint table (Cargo cannot combine `[lints] workspace = true` with
  a local override); it declares `unsafe_code = "allow"`, keeps
  `unsafe_op_in_unsafe_fn = "deny"`, and reproduces the workspace Clippy table
  verbatim. Every other crate keeps the inherited `unsafe_code = "forbid"`.
- Every `unsafe` block either wraps exactly one documented OS call and carries
  an English SAFETY comment naming the descriptor/buffer contract it relies on,
  or is one of the two typed field-read/classification blocks in the credential
  DACL walk (`windows.rs` `grants_only_to_trusted_principals`): reading the
  leading `ACE_HEADER` of a `GetAce`-returned record and the `ACCESS_ALLOWED_ACE`
  fields of a record classified as access-allowed. This is the crate's only
  approved non-call unsafe exception (owner-scoped decision D44). Each such
  block carries an English SAFETY comment citing the exact documented record
  contract it relies on — layout (`ACE_HEADER` first member of every ACE;
  `ACCESS_ALLOWED_ACE` `{Header, Mask, SidStart}` with the SID contiguous after
  the mask), extent (`GetAce` succeeds for indices below `AceCount`; the
  checked `AceSize` covers the full field layout before the allowed-record
  read), alignment (each ACE begins on a DWORD boundary) and lifetime (the
  records live inside the descriptor captured above, which outlives the walk) —
  and the walk refuses records it cannot interpret.
- The exception authorizes no general security-descriptor/ACL byte parser and
  no reinterpretation of unknown layouts: unknown or unsupported policy still
  refuses, public functions stay safe, and no unsafe memory operation may hide
  inside an OS-call block or a helper.
- Public functions are safe. They never decide whether to publish: a failure is
  returned so the caller refuses and leaves the old document unchanged.
- No hand-rolled security parsing: use the platform facilities directly
  (`libc` on Unix including the macOS `acl_*` family, `windows-sys`
  `GetFileSecurityW`/`SetSecurityInfo` on Windows).
- Staging happens in a private directory that is created and sealed before the
  staging file: on Unix the directory's inherited ACL/default ACL is cleared and
  its owner-only mode is re-established and validated, on Windows the file is
  opened with no sharing, so no staged byte is exposed before the captured
  policy is applied.
- Dependencies stay at the already-locked `libc`/`windows-sys` versions; no new
  native library, runtime option or third-party wrapper.
- A platform with neither contract must fail closed rather than publish without
  the target's policy.
