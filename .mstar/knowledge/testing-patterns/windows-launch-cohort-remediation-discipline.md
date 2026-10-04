---
module: nexus-agent-host
date: 2026-10-03
problem_type: testing_pattern
category: testing-patterns
severity: medium
plan_id: 2026-10-02-v1.204-p1-windows-launch-readiness-parity
tags:
  - windows
  - launch-cohort
  - assertion-strength
  - fixture-cwd
  - crlf-asset-pin
  - pathext
  - platform-expectation-predicate
  - evidence-provenance
applies_when:
  - remediating a red Windows platform-behaviour cohort after a selector extension
  - writing platform-split assertions (cfg gates, platform-expectation predicates)
  - reviewing a platform-gated diff where a test was "made green" by gating or relaxing an assertion
---

# Windows Launch-Cohort Remediation Discipline (assertions survive the fix)

## Context

Iteration v1.204 consumed residual `R-V1202-P1T3-001`: the `platform-behaviour.yml` Windows leg was extended from the `map_`-only subset to the full `providers::native_cli` cohort plus a readiness-acceptance step, converting the v1.202/v1.203 triage hypotheses ([windows-fixture-launch-cmd-wrapper-seam.md](windows-fixture-launch-cmd-wrapper-seam.md)) into a measured fix-or-disposition pass. First full run: 80 passed / **39 failed** lib tests + 2/4 readiness. Final run: **112 passed / 0 failed** + 7/0, with every remaining exclusion a per-test `#[cfg(unix)]` gate naming a register row + trigger. Two QC rounds then caught the two ways this kind of pass can still be a lie (weak assertions, wrong evidence provenance).

## Guidance

1. **Mine the baseline before fixing anything.** Extend the selector first, capture the current-HEAD run's per-test list byte-preserved into a durable artifact (run URL + job id + head sha + verbatim log lines), and only then classify. Classification is triage hypotheses until the run confirms them; keep the failed intermediate runs distinct from the green ones in every artifact.

2. **Invalid temp cwd is the Windows launch killer — fix it with `std::env::temp_dir()`.** The dominant class (27/39) was fixtures spawned with a unix `/tmp` literal working directory: Windows fails the spawn with `ERROR_DIRECTORY` (267) **before the fixture ever starts** (no fixture log lines at all). Portable replacements: `std::env::temp_dir()`; never a hard-coded unix path literal.

3. **`include_str!` asset pins drift under CRLF checkouts.** An `include_str!`-embedded asset is hashed from **working-tree bytes**; a Windows checkout with `autocrlf` converts them to CRLF and the SHA-256 pin fails though the source is unchanged. Normalize line endings (LF) before the comparison and keep the reference digest unchanged — never re-pin to the drifted value.

4. **Canonicalize before comparing recorded vs expected paths.** Windows returns 8.3 short names and `\\?\` verbatim prefixes; `std::fs::canonicalize` / `std::path::absolute` both sides before equality. Same rule for PATH-discovery fixtures: a bare command resolution needs the real `PATHEXT`-suffixed stub files written (e.g. `dsh` + `dsh.cmd`), not just an extensionless file.

5. **The fix must not weaken the assertion (the v1.204 double-failure example).** A route-acceptance helper was first "fixed" to accept a nonempty fixture log; QC showed the fixture writes `_spawn` before reading any request, so a startup-only run passed vacuously. The strengthened form requires **protocol receipts** (`initialize` request + cooperative `shutdown` close) **and** a platform-expectation predicate over the full health result: unix demands `available`; Windows demands the specific unsupported-sealed-provisioning reason, which the probe emits **only after** the ordinary recipe's confirmed start and close. "Accepts any unavailable" is the false-pass shape — reject it in review.

6. **Gate the injected setup, not the failure-path assertions.** `sealed_provision_failure_closes_session_permanently` was initially whole-test `#[cfg(unix)]`-gated; but its invariants (launch-class error, later executes rejected, exactly one ordinary spawn, zero prompt admission, retained closed state, successful final shutdown) are exercised on Windows **precisely because** unsupported provisioning returns `Err` there. Keep only the filesystem-blocker injection unix-only and let the failure-state assertions run on every platform — an over-broad gate deletes real Windows-reachable regression coverage (a security-adjacent lifecycle, not just sealed success).

7. **Real probe, never a constant stub.** `process_alive` on Windows is a real `tasklist /FI "PID eq <pid>" /FO CSV /NH` CSV-column parse (localization-independent; exit status is 0 either way, so parse output, never status); spawn failure or non-zero exit panics with the probe command named. A stub that silently reports "not alive" makes every `!process_alive` leak assertion pass vacuously.

8. **Provenance discipline across run waves.** With multiple runs (baseline → failed intermediate → first green → strengthened-assertion green → final), the register row and durable artifact must name exactly one **final** run — the reviewed tip's green run — and keep earlier failures as history. A register row citing a failed intermediate as "FINAL verification" is an acceptance-artifact defect, not stale wording (caught by two QC seats in v1.204).

## Why This Matters

Platform parity work fails in two seductive ways: **faking it** (green via weakened assertions, catch-all filters, or `#[ignore]`) and **over-gating it** (deleting Windows-reachable coverage to make a platform quirk disappear). Both turn the CI leg into a green light that detects nothing. The discipline above keeps the signal: every fix preserves assertion strength, every exclusion is a source-level per-test gate with a register row and a named trigger, and the executed subset stays auditable against the no-catch-all rule.

## When to Apply

- Remediating a red platform cohort after extending a CI selector (this document is the v1.204 playbook; [windows-fixture-launch-cmd-wrapper-seam.md](windows-fixture-launch-cmd-wrapper-seam.md) is the v1.203 seam/hypothesis substrate it consumed).
- Writing or reviewing any platform-split assertion: prefer platform-expectation predicates over dropped checks; gate setup, not invariants.
- Any fix round whose diff "makes a platform test green" — verify the assertion did not get weaker.

## Examples

```rust
// Rule 2 — portable fixture cwd
let cwd = std::env::temp_dir(); // not /tmp

// Rule 3 — CRLF-tolerant asset pin (reference digest unchanged)
let bytes = include_str!("../../assets/deny_all.yaml");
let normalized = bytes.replace("\r\n", "\n");
assert_eq!(sha256_hex(normalized.as_bytes()), EXPECTED_LF_DIGEST);

// Rule 5 — platform-expectation predicate (route acceptance)
match expected {
    Platform::Unix   => assert!(health.available, "unix must reach availability"),
    Platform::Windows => assert!(
        !health.available
            && health.reason.contains("sealed deny_all home provisioning is unsupported"),
        "windows only admits the post-confirmed-ordinary-close sealed limitation",
    ),
}
```

## See also

- [windows-fixture-launch-cmd-wrapper-seam.md](windows-fixture-launch-cmd-wrapper-seam.md) — the launcher seam and triage hypotheses this pass measured and consumed
- [process-env-lock-fixture-spawn-serialization.md](../workflow-patterns/process-env-lock-fixture-spawn-serialization.md) — env-lock contract the fixtures hold during construction
- [branch-diff-vs-worktree-state.md](../workflow-patterns/branch-diff-vs-worktree-state.md) — related review-evidence discipline

> Consolidation note: moderate overlap with `windows-fixture-launch-cmd-wrapper-seam.md` (seam mechanics vs remediation/assertion discipline). Candidate for merge on a future compound-refresh pass if the seam document shrinks to pure mechanics.
