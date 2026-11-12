#!/usr/bin/env bash
# ---------------------------------------------------------------------------
# check-work-artifact-writes.sh — Work artifact byte-write guard (ADR #485
# §6.1 P5, v1.236 P5-T2).
#
# Static scan of production source in the P0/P2 inventoried Work-writer
# modules. Direct `std::fs::write`, `tokio::fs::write` and imported `fs::write`
# calls there mean a second Work writer family — the port (FsWorkArtifactPort /
# commit_work_bytes) is the only production Work byte path.
#
# Classification rules (mirrors .mstar/sdd/v234-work-artifact-ir-p0-p1/inventory.md):
#   - production source = every line before the file's first `#[cfg(test)]`
#     (or `#[cfg(all(test, …))]`); later hits are fixtures.
#   - one exact (path, symbol) pair is the port-routed non-Work fallback: the
#     guarded-write else-branch of `execute_write_file` (EXACT_FALLBACK below).
#     It is reported, never silently ignored.
#   - every other direct write is a violation unless the exact
#     path<TAB>symbol<TAB>disposition row exists in the allowlist.
#
# Exemption mechanism: tooling/work-artifact-write-allowlist.tsv. Exact
# path+symbol only — never a module, line-number or comment waiver. A stale
# row (no matching write) fails the guard.
#
# Usage (from repository root):
#   bash tooling/check-work-artifact-writes.sh                 # scan inventory
#   bash tooling/check-work-artifact-writes.sh <path|file.rs>  # scan a fixture
#
# Exit codes: 0 — no unexempted direct Work write; 1 — violation / bad input.
# ---------------------------------------------------------------------------
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export ROOT
cd "$ROOT"

python3 - "$@" <<'PY'
import os
import re
import sys

root = os.environ["ROOT"]
allowlist_path = os.path.join(root, "tooling", "work-artifact-write-allowlist.tsv")

# The P0/P2 inventoried Work-writer modules (14 modules; rules_history.rs was
# removed in P2 and is absent here on purpose).
INVENTORY = [
    "crates/nexus-core/src/execution/capabilities.rs",
    "crates/nexus-local-db/src/work_chapters.rs",
    "crates/nexus-orchestration/src/completion_lock.rs",
    "crates/nexus-orchestration/src/narrative_index.rs",
    "crates/nexus-orchestration/src/quality_loop.rs",
    "crates/nexus-orchestration/src/rules_layers.rs",
    "crates/nexus-orchestration/src/capability/builtins/essay_scaffold.rs",
    "crates/nexus-orchestration/src/capability/builtins/game_bible_scaffold.rs",
    "crates/nexus-orchestration/src/capability/builtins/game_bible_section_status.rs",
    "crates/nexus-orchestration/src/capability/builtins/novel_scaffold.rs",
    "crates/nexus-orchestration/src/capability/builtins/novel_chapter_transition.rs",
    "crates/nexus-orchestration/src/capability/builtins/script_scaffold.rs",
    "crates/nexus-orchestration/src/capability/builtins/essay_draft_status.rs",
]

# shortcut: only `fs::write` spellings are scanned; `File::create(..).write_all(..)`
# and `fs::copy` are out of scope until a writer is observed using them.
WRITE_RE = re.compile(r"(?<![\w])(?:std::fs|tokio::fs|fs)::write\s*\(")

# The one non-Work direct write that is allowed without an allowlist row: it is
# the guarded-write else-branch of `execute_write_file`, which routes every
# `WorkPath::Artifact` to the port before reaching this call. This is an exact
# (path, symbol) pair — never a function-wide property.
EXACT_FALLBACK = {
    ("crates/nexus-core/src/execution/capabilities.rs", "execute_write_file"),
}
TEST_ATTR_RE = re.compile(r"^\s*#\[cfg\((?:all\()?test")
FN_RE = re.compile(r"\bfn\s+([A-Za-z_]\w*)")
IMPL_FOR_RE = re.compile(r"\bimpl(?:\s*<[^>]*>)?\s+[^{]*?\bfor\s+([A-Za-z_]\w*)")
IMPL_RE = re.compile(r"\bimpl(?:\s*<[^>]*>)?\s+([A-Za-z_]\w*)")

errors = []


def fail(msg):
    errors.append(msg)


def load_allowlist():
    rows = {}
    with open(allowlist_path, encoding="utf-8") as fh:
        for num, line in enumerate(fh, 1):
            line = line.rstrip("\n")
            if not line.strip() or line.lstrip().startswith("#"):
                continue
            parts = line.split("\t")
            if len(parts) != 3 or not all(parts):
                fail(f"{allowlist_path}:{num}: expected path<TAB>symbol<TAB>disposition")
                continue
            key = (parts[0], parts[1])
            if key in rows:
                fail(f"{allowlist_path}:{num}: duplicate exemption for {key[0]} {key[1]}")
                continue
            rows[key] = parts[2]
    return rows


def production_prefix(lines):
    for idx, line in enumerate(lines):
        if TEST_ATTR_RE.match(line):
            return lines[:idx]
    return lines


def symbol_at(lines, idx):
    """Enclosing `Impl::fn` (or bare `fn`) for the 0-based hit line."""
    fn = impl = None
    impl_locked = False
    for j in range(idx - 1, -1, -1):
        line = lines[j]
        if fn is None:
            m = FN_RE.search(line)
            if m:
                fn = m.group(1)
        if impl is None and not impl_locked:
            m = IMPL_FOR_RE.search(line) or IMPL_RE.search(line)
            if m:
                impl = m.group(1)
            elif line.startswith("}"):
                # Column-0 `}` closes a top-level item: any earlier `impl` is a
                # sibling, not an ancestor.
                impl_locked = True
        if fn and impl:
            break
    if impl and fn:
        return f"{impl}::{fn}"
    return fn or "<unknown>"


def scan_file(path):
    with open(path, encoding="utf-8") as fh:
        lines = fh.read().splitlines()
    rel = os.path.relpath(os.path.realpath(path), os.path.realpath(root))
    hits = []
    for idx, line in enumerate(production_prefix(lines)):
        if WRITE_RE.search(line):
            hits.append((idx, line.strip(), symbol_at(lines, idx)))
    return rel, hits


def main(argv):
    allow = load_allowlist()

    if argv:
        targets = []
        for arg in argv:
            if os.path.isdir(arg):
                for dirpath, _dirs, files in os.walk(arg):
                    targets.extend(
                        os.path.join(dirpath, f) for f in sorted(files) if f.endswith(".rs")
                    )
            elif os.path.isfile(arg):
                targets.append(arg)
            else:
                fail(f"no such path: {arg}")
        if not targets:
            fail("nothing to scan")
    else:
        targets = INVENTORY
        missing = [p for p in INVENTORY if not os.path.isfile(p)]
        for p in missing:
            fail(f"inventoried module missing: {p}")

    seen_rows = set()
    exempt = 0
    fallback = 0
    violations = 0

    for target in targets:
        rel, hits = scan_file(target)
        for _idx, text, symbol in hits:
            key = (rel, symbol)
            if key in allow:
                seen_rows.add(key)
                exempt += 1
                print(f"exempt: {rel} {symbol} — {allow[key]}")
            elif key in EXACT_FALLBACK:
                fallback += 1
                print(f"non-Work fallback (Work paths routed to the port): {rel} {symbol}")
            else:
                violations += 1
                print(f"::error::work artifact direct write: {rel} {symbol} — {text}")

    if not argv:
        for key in sorted(set(allow) - seen_rows):
            fail(
                "stale allowlist row (no matching direct write): "
                f"{key[0]} {key[1]}"
            )

    for msg in errors:
        print(f"::error::{msg}")

    if violations or errors:
        print(
            f"\nWork artifact write guard FAILED: {violations} unexempted direct "
            f"write(s), {len(errors)} guard/allowlist error(s). Route Work bytes "
            "through the Work artifact port, or add an exact allowlist row with a "
            "disposition."
        )
    else:
        print(
            f"\nWork artifact write guard OK: {exempt} exact-symbol exemption(s), "
            f"{fallback} port-routed non-Work fallback(s)."
        )

    return 1 if (violations or errors) else 0


sys.exit(main(sys.argv[1:]))
PY
