#!/usr/bin/env bash
# ---------------------------------------------------------------------------
# check-work-artifact-writes.sh — Work artifact byte-write guard (ADR #485
# §6.1 P5, v1.236 P5-T2).
#
# Static scan of production source in the inventoried Work-writer modules.
# Direct `std::fs::write`, `tokio::fs::write` and imported `fs::write` calls
# there mean a second Work writer family — the port (FsWorkArtifactPort /
# commit_work_bytes) is the only production Work byte path.
#
# Inventory (F-003): the six named top-level modules plus a glob of
# `crates/nexus-orchestration/src/capability/builtins/*.rs`. Every globbed
# file is scanned by default, so a new builtin writer is covered without an
# inventory edit. A file can only be skipped by an explicit opt-out row
# (`path<TAB>-<TAB>0<TAB>disposition`) in the allowlist.
#
# Classification (F-002): a `#[cfg(test)]` line cuts production only when it
# annotates a `mod` (the test module span is excluded); the scan otherwise runs
# to EOF. A `#[cfg(test)]` on a non-module item (e.g. a test-only const) does
# not hide production lines below it. An unclassifiable annotation fails closed.
#
# Exemptions (F-001): tooling/work-artifact-write-allowlist.tsv rows are
# `path<TAB>symbol<TAB>expected_count<TAB>disposition` — exact path+symbol and
# an exact observed hit count. No module, line-number or comment waiver. A
# stale row, a duplicate row or a count mismatch fails the guard.
#
# shortcut: only `fs::write` spellings are scanned; `File::create(..).write_all(..)`
# and `fs::copy` are out of scope until a writer is observed using them.
#
# Usage (from repository root):
#   bash tooling/check-work-artifact-writes.sh                 # scan inventory
#   bash tooling/check-work-artifact-writes.sh <path|file.rs>  # scan a fixture
# Fixture targets resolve allowlist paths against the target directory, so a
# fixture tree can mirror an inventoried module.
#
# Exit codes: 0 — no unexempted direct Work write; 1 — violation / bad input.
# ---------------------------------------------------------------------------
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export ROOT
cd "$ROOT"

python3 - "$@" <<'PY'
import glob
import os
import re
import sys

root = os.environ["ROOT"]
allowlist_path = os.path.join(root, "tooling", "work-artifact-write-allowlist.tsv")

# Named top-level inventoried modules (rules_history.rs was removed in P2).
NAMED = [
    "crates/nexus-core/src/execution/capabilities.rs",
    "crates/nexus-local-db/src/work_chapters.rs",
    "crates/nexus-orchestration/src/completion_lock.rs",
    "crates/nexus-orchestration/src/narrative_index.rs",
    "crates/nexus-orchestration/src/quality_loop.rs",
    "crates/nexus-orchestration/src/rules_layers.rs",
]
BUILTINS_GLOB = "crates/nexus-orchestration/src/capability/builtins/*.rs"

WRITE_RE = re.compile(r"(?<![\w])(?:std::fs|tokio::fs|fs)::write\s*\(")
TEST_ATTR_RE = re.compile(r"^\s*#\[cfg\((?:all\()?test")
MOD_RE = re.compile(r"^\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+[A-Za-z_]\w*")
ITEM_RE = re.compile(
    r"^\s*(?:pub(?:\([^)]*\))?\s+)?"
    r"(?:const|static|fn|use|struct|enum|impl|trait|type|extern|macro_rules!)\b"
)
FN_RE = re.compile(r"\bfn\s+([A-Za-z_]\w*)")
IMPL_FOR_RE = re.compile(r"\bimpl(?:\s*<[^>]*>)?\s+[^{]*?\bfor\s+([A-Za-z_]\w*)")
IMPL_RE = re.compile(r"\bimpl(?:\s*<[^>]*>)?\s+([A-Za-z_]\w*)")

# The one non-Work direct write allowed without an allowlist row: the
# guarded-write else-branch of `execute_write_file`, which routes every
# `WorkPath::Artifact` to the port before reaching this call. Exact
# (path, symbol) -> expected count; any other function is a violation.
EXACT_FALLBACK = {
    ("crates/nexus-core/src/execution/capabilities.rs", "execute_write_file"): 1,
}

OPT_OUT_SYMBOL = "-"

errors = []


def fail(msg):
    errors.append(msg)


def load_allowlist():
    """path -> {symbol: (expected_count, disposition)} plus the opt-out set."""
    rows = {}
    opt_outs = set()
    with open(allowlist_path, encoding="utf-8") as fh:
        for num, line in enumerate(fh, 1):
            line = line.rstrip("\n")
            if not line.strip() or line.lstrip().startswith("#"):
                continue
            parts = line.split("\t")
            if len(parts) != 4 or not all(parts):
                fail(f"{allowlist_path}:{num}: expected path<TAB>symbol<TAB>count<TAB>disposition")
                continue
            path, symbol, count_text, disposition = parts
            try:
                expected = int(count_text)
            except ValueError:
                fail(f"{allowlist_path}:{num}: expected_count is not an integer: {count_text!r}")
                continue
            key = (path, symbol)
            if symbol in rows.get(path, {}):
                fail(f"{allowlist_path}:{num}: duplicate exemption for {path} {symbol}")
                continue
            if symbol == OPT_OUT_SYMBOL:
                if expected != 0:
                    fail(f"{allowlist_path}:{num}: opt-out row must use count 0")
                    continue
                opt_outs.add(path)
            rows.setdefault(path, {})[symbol] = (expected, disposition)
    return rows, opt_outs


def enclosing_symbol(lines, idx):
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


def test_module_spans(rel, lines):
    """Line indexes belonging to `#[cfg(test)] mod …` blocks."""
    skip = set()
    i = 0
    while i < len(lines):
        if not TEST_ATTR_RE.match(lines[i]):
            i += 1
            continue
        j = i + 1
        while j < len(lines) and (
            not lines[j].strip()
            or lines[j].lstrip().startswith("#[")
            or lines[j].lstrip().startswith("//")
        ):
            j += 1
        if j >= len(lines):
            fail(f"{rel}:{i + 1}: #[cfg(test)] at EOF — cannot classify the item")
            i += 1
            continue
        if MOD_RE.match(lines[j]):
            open_line = j
            while open_line < len(lines) and "{" not in lines[open_line]:
                open_line += 1
            if open_line >= len(lines):
                fail(f"{rel}:{i + 1}: #[cfg(test)] mod without a body")
                i = j + 1
                continue
            indent = len(lines[j]) - len(lines[j].lstrip())
            end = None
            for m in range(open_line + 1, len(lines)):
                stripped = lines[m].lstrip()
                if not stripped:
                    continue
                if stripped.startswith("}") and (len(lines[m]) - len(stripped)) <= indent:
                    end = m
                    break
            if end is None:
                fail(f"{rel}:{i + 1}: unclosed #[cfg(test)] mod")
                i = j + 1
                continue
            skip.update(range(i, end + 1))
            i = end + 1
            continue
        if ITEM_RE.match(lines[j]):
            # Test-only non-module item (e.g. a ledger const): not a cut point,
            # so production below it stays visible.
            i += 1
            continue
        fail(f"{rel}:{i + 1}: ambiguous #[cfg(test)] annotation — cannot classify the item")
        i += 1
    return skip


def scan_file(path, base):
    with open(path, encoding="utf-8") as fh:
        lines = fh.read().splitlines()
    rel = os.path.relpath(os.path.realpath(path), os.path.realpath(base))
    skip = test_module_spans(rel, lines)
    hits = []
    for idx, line in enumerate(lines):
        if idx in skip:
            continue
        # Count every call, not every matching line: two direct writes on one
        # line must register as two hits.
        occurrences = len(WRITE_RE.findall(line))
        if occurrences:
            symbol = enclosing_symbol(lines, idx)
            hits.extend([(idx, line.strip(), symbol)] * occurrences)
    return rel, hits


def collect_targets(argv, opt_outs):
    if argv:
        targets = []
        for arg in argv:
            if os.path.isdir(arg):
                base = arg
                for dirpath, _dirs, files in os.walk(arg):
                    targets.extend(
                        (os.path.join(dirpath, f), base)
                        for f in sorted(files)
                        if f.endswith(".rs")
                    )
            elif os.path.isfile(arg):
                targets.append((arg, os.path.dirname(arg) or "."))
            else:
                fail(f"no such path: {arg}")
        if not targets:
            fail("nothing to scan")
        return targets

    targets = []
    for path in NAMED:
        if not os.path.isfile(path):
            fail(f"inventoried module missing: {path}")
        elif path in opt_outs:
            fail(f"named inventoried module cannot be opted out: {path}")
        else:
            targets.append((path, root))

    globbed = sorted(glob.glob(BUILTINS_GLOB))
    if not globbed:
        fail(f"inventoried glob matched nothing: {BUILTINS_GLOB}")
    for path in globbed:
        rel = os.path.relpath(os.path.realpath(path), os.path.realpath(root))
        if rel in opt_outs:
            print(f"opt-out (allowlist): {rel}")
            continue
        targets.append((path, root))
    return targets


def main(argv):
    allow, opt_outs = load_allowlist()
    targets = collect_targets(argv, opt_outs)

    observed = {}
    violations = 0

    for path, base in targets:
        rel, hits = scan_file(path, base)
        for _idx, text, symbol in hits:
            key = (rel, symbol)
            observed[key] = observed.get(key, 0) + 1
            exempt = symbol in allow.get(rel, {})
            if not exempt and key not in EXACT_FALLBACK:
                violations += 1
                print(f"::error::work artifact direct write: {rel} {symbol} — {text}")

    exempt = 0
    fallback = 0
    for (rel, symbol), count in sorted(observed.items()):
        if symbol in allow.get(rel, {}):
            expected, disposition = allow[rel][symbol]
            if count == expected:
                exempt += 1
                print(f"exempt: {rel} {symbol} ×{count} — {disposition}")
            else:
                fail(f"{rel} {symbol}: observed {count} direct write(s), expected {expected}")
        elif (rel, symbol) in EXACT_FALLBACK:
            expected = EXACT_FALLBACK[(rel, symbol)]
            if count == expected:
                fallback += 1
                print(f"non-Work fallback (Work paths routed to the port): {rel} {symbol} ×{count}")
            else:
                fail(f"{rel} {symbol}: non-Work fallback observed {count}, expected {expected}")

    if not argv:
        for rel, symbols in sorted(allow.items()):
            for symbol, (expected, _disposition) in symbols.items():
                if symbol == OPT_OUT_SYMBOL:
                    continue
                if (rel, symbol) not in observed:
                    fail(f"stale allowlist row (no matching direct write): {rel} {symbol} (expected {expected})")
        for (rel, symbol), expected in EXACT_FALLBACK.items():
            if (rel, symbol) not in observed:
                fail(f"non-Work fallback row did not match: {rel} {symbol} (expected {expected})")

    for msg in errors:
        print(f"::error::{msg}")

    if violations or errors:
        print(
            f"\nWork artifact write guard FAILED: {violations} unexempted direct "
            f"write(s), {len(errors)} guard/allowlist error(s). Route Work bytes "
            "through the Work artifact port, or add an exact allowlist row with a "
            "disposition and count."
        )
    else:
        print(
            f"\nWork artifact write guard OK: {exempt} exact-symbol exemption(s), "
            f"{fallback} port-routed non-Work fallback(s)."
        )

    return 1 if (violations or errors) else 0


try:
    sys.exit(main(sys.argv[1:]))
except SystemExit:
    raise
except Exception as exc:  # F-005: annotate, never dump a traceback
    print(f"::error::work artifact write guard crashed: {exc!r}")
    sys.exit(1)
PY
