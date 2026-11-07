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
- Every `unsafe` block wraps exactly one documented OS call and carries an
  English SAFETY comment naming the descriptor/buffer contract it relies on.
- Public functions are safe. They never decide whether to publish: a failure is
  returned so the caller refuses and leaves the old document unchanged.
- No hand-rolled security parsing: use the platform facilities directly
  (`libc` on Unix including the macOS `acl_*` family, `windows-sys`
  `GetFileSecurityW`/`SetSecurityInfo` on Windows).
- Staging happens in a private directory that is created and sealed before the
  staging file: on Unix the directory's inherited ACL/default ACL is cleared, on
  Windows the file is opened with no sharing, so no staged byte is exposed
  before the captured policy is applied.
- Dependencies stay at the already-locked `libc`/`windows-sys` versions; no new
  native library, runtime option or third-party wrapper.
- A platform with neither contract must fail closed rather than publish without
  the target's policy.
