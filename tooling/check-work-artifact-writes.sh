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
# `crates/nexus-capability/src/builtins/*.rs` (the capability tree moved
# there in v249 C2; the old orchestration paths no longer exist). Every globbed
# file is scanned by default, so a new builtin writer is covered without an
# inventory edit. A file can only be skipped by an explicit opt-out row
# (`path<TAB>-<TAB>0<TAB>disposition`) in the allowlist.
#
# Classification (F-002): only predicates that require `test` compilation are
# gates: `#[cfg(test)]` and `#[cfg(all(…, test))]` (a trailing comma in the
# predicate list is ignored). `#[cfg(any(test, …))]` is not a gate — another
# disjunct (e.g. a production feature) can hold without test — and neither is
# `#[cfg(not(test))]` or `#[cfg(feature = "test")]`. A gate cuts a span only
# for an item whose `{ … }` body the tracker can match: a `mod … { }` or a
# macro invocation (`mac!`, `path::mac!`, `::mac!`). A `mod … { }` body is
# delimited by indentation (unchanged); a macro body by brace depth, ignoring
# `{`/`}` inside literals and comments, so a body that opens and closes on one
# line or is misindented is matched exactly, and an unbalanced one fails closed.
# An out-of-line `mod tests;` opens no span (its body is another file), and a
# gate on a non-module item (e.g. a test-only const), struct field or statement
# does not hide production lines below it. An opaque body the tracker cannot
# delimit (a bare `{ … }` block or a non-brace macro invocation such as
# `include!(…)`) fails closed.
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
    "crates/nexus-core/src/owner/capabilities.rs",
    "crates/nexus-local-db/src/work_chapters.rs",
    "crates/nexus-orchestration/src/completion_lock.rs",
    "crates/nexus-orchestration/src/narrative_index.rs",
    "crates/nexus-orchestration/src/quality_loop.rs",
    "crates/nexus-capability/src/rules_layers.rs",
]
BUILTINS_GLOB = "crates/nexus-capability/src/builtins/*.rs"

WRITE_RE = re.compile(r"(?<![\w])(?:std::fs|tokio::fs|fs)::write\s*\(")
CFG_ATTR_RE = re.compile(r"^\s*#\[cfg\((.*)\)\]")
# A raw or byte-raw string opener (`r"…"`, `r#"…"#`, `br#"…"#`): braces inside
# it do not count toward a macro body's depth.
RAW_STRING_RE = re.compile(r'b?r(#*)"')
MOD_RE = re.compile(r"^\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+[A-Za-z_]\w*")
# An out-of-line `mod tests;` declaration: the body lives in another file, so it
# opens no span here.
MOD_DECL_RE = re.compile(r"^\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+[A-Za-z_]\w*\s*;")
# A test-gated macro invocation with a `{ … }` token tree (`mac!`, `path::mac!`,
# `::mac!`): the tracker can match its closing brace, so the body is excluded
# like a `mod … { }` span.
MACRO_BODY_RE = re.compile(r"^\s*(?:::)?(?:[A-Za-z_]\w*::)*[A-Za-z_]\w*!\s*\{")
ITEM_RE = re.compile(
    r"^\s*(?:pub(?:\([^)]*\))?\s+)?"
    r"(?:const|static|fn|use|struct|enum|impl|trait|type|extern|macro_rules!)\b"
)
# A test-gated attribute on an opaque body (a bare block, or a macro invocation
# without a `{ … }` body) could hide a module the span tracker cannot follow, so
# the guard fails closed instead of scanning past it.
OPAQUE_RE = re.compile(r"^\s*(?:\{|(?:::)?(?:[A-Za-z_]\w*::)*[A-Za-z_]\w*!)")
FN_RE = re.compile(r"\bfn\s+([A-Za-z_]\w*)")
IMPL_FOR_RE = re.compile(r"\bimpl(?:\s*<[^>]*>)?\s+[^{]*?\bfor\s+([A-Za-z_]\w*)")
IMPL_RE = re.compile(r"\bimpl(?:\s*<[^>]*>)?\s+([A-Za-z_]\w*)")

# The one non-Work direct write allowed without an allowlist row: the
# guarded-write else-branch of `execute_write_file`, which routes every
# `WorkPath::Artifact` to the port before reaching this call. Exact
# (path, symbol) -> expected count; any other function is a violation.
EXACT_FALLBACK = {
    ("crates/nexus-core/src/owner/capabilities.rs", "execute_write_file"): 1,
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


def split_top_level(expr):
    """Comma-separated non-empty items of `expr` at paren depth 0."""
    parts, depth, buf = [], 0, []
    for ch in expr:
        if depth == 0 and ch == ",":
            parts.append("".join(buf))
            buf = []
            continue
        if ch == "(":
            depth += 1
        elif ch == ")":
            depth -= 1
        buf.append(ch)
    parts.append("".join(buf))
    # A trailing comma (`any(test,)`) or a stray comma leaves an empty part.
    return [part for part in parts if part.strip()]


def requires_test(expr):
    """True only when the predicate set holds *only* under test compilation.

    `test` and a conjunct of `all(..)` require it; `any(..)` requires it only
    when every disjunct does, so `any(test, feature = ..)` never counts.
    """
    parts = split_top_level(expr)
    if len(parts) > 1:
        # `cfg(a, b)` is not valid Rust; require test from every part.
        return all(requires_test(part) for part in parts)
    if not parts:
        return False
    m = re.match(r"\s*([A-Za-z_]\w*)\s*(?:\((.*)\))?\s*(=.*)?$", parts[0])
    if not m:
        return False
    name, inner, is_key = m.group(1), m.group(2), m.group(3)
    if is_key is not None:
        # `key = "value"` predicate (e.g. `feature = "test"`): never a gate.
        return False
    if inner is None:
        return name == "test"
    if name == "all":
        return any(requires_test(item) for item in split_top_level(inner))
    if name == "any":
        disjuncts = split_top_level(inner)
        # Empty `any()` never holds; otherwise every disjunct must require test.
        return bool(disjuncts) and all(requires_test(item) for item in disjuncts)
    return False


def test_gate_match(line):
    """The `#[cfg(..)]` match when the line compiles only under test, else None."""
    m = CFG_ATTR_RE.match(line)
    if not m or not requires_test(m.group(1)):
        return None
    return m


def quoted_end(text, quote_at, quote):
    """Index just past the literal opened at `quote_at`, or `len(text)` if open."""
    k = quote_at + 1
    while k < len(text):
        if text[k] == "\\":
            k += 2
            continue
        if text[k] == quote:
            return k + 1
        k += 1
    return len(text)


def literal_end(text, k):
    """Index just past a string/char literal starting at `k`, else None.

    A lifetime (`'a`) is not a char literal, so scanning passes through it.
    """
    raw = RAW_STRING_RE.match(text, k)
    if raw:
        closing = '"' + raw.group(1)
        end = text.find(closing, raw.end())
        return len(text) if end < 0 else end + len(closing)
    if text.startswith('b"', k):
        return quoted_end(text, k + 1, '"')
    if text[k] == '"':
        return quoted_end(text, k, '"')
    if text[k] == "'":
        if k + 1 < len(text) and text[k + 1] == "\\":
            return quoted_end(text, k, "'")
        if k + 2 < len(text) and text[k + 2] == "'":
            return k + 3
    return None


def depth_body_end(lines, first, start_col):
    """Last line of the `{ … }` body opening at `lines[first][start_col:]`.

    Depth, not indentation: the body may open and close on this line, and its
    content need not be indented. A multi-line literal or block comment is not
    followed across lines, so a stray brace there can only shorten the span.
    """
    depth = 0
    in_block_comment = False
    for idx in range(first, len(lines)):
        text = lines[idx]
        k = start_col if idx == first else 0
        while k < len(text):
            if in_block_comment:
                if text.startswith("*/", k):
                    in_block_comment = False
                    k += 2
                    continue
                k += 1
                continue
            if text.startswith("//", k):
                break
            if text.startswith("/*", k):
                in_block_comment = True
                k += 2
                continue
            end = literal_end(text, k)
            if end is not None:
                k = end
                continue
            if text[k] == "{":
                depth += 1
            elif text[k] == "}":
                depth -= 1
                if depth == 0:
                    return idx
                if depth < 0:
                    return None
            k += 1
    return None


def test_module_spans(rel, lines):
    """Line indexes belonging to test-gated `mod … { }` / `mac! { }` spans."""
    skip = set()
    i = 0
    while i < len(lines):
        attr = test_gate_match(lines[i])
        if attr is None:
            i += 1
            continue
        # The gated item may start on the attribute's own line
        # (`#[cfg(test)] mac! { .. }`).
        rest = lines[i][attr.end():]
        same_line = bool(rest.strip()) and not rest.lstrip().startswith("//")
        if same_line:
            j = i
            item = rest
        else:
            j = i + 1
            while j < len(lines) and (
                not lines[j].strip()
                or lines[j].lstrip().startswith("#[")
                or lines[j].lstrip().startswith("//")
            ):
                j += 1
            if j >= len(lines):
                fail(f"{rel}:{i + 1}: test-gated attribute at EOF — cannot classify the item")
                i += 1
                continue
            item = lines[j]
        if MOD_DECL_RE.match(item):
            # Out-of-line `mod tests;`: the body is another file, so this
            # declaration opens no span and the code below stays production.
            i += 1
            continue
        if MOD_RE.match(item):
            # Module bodies keep the indentation heuristic.
            open_line = j
            while open_line < len(lines) and "{" not in lines[open_line]:
                open_line += 1
            if open_line >= len(lines):
                fail(f"{rel}:{i + 1}: test-gated item without a body")
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
                fail(f"{rel}:{i + 1}: unclosed test-gated body")
                i = j + 1
                continue
            skip.update(range(i, end + 1))
            i = end + 1
            continue
        if MACRO_BODY_RE.match(item):
            # Macro-invocation bodies are delimited by brace depth.
            start_col = item.index("{") if j > i else attr.end() + item.index("{")
            end = depth_body_end(lines, j, start_col)
            if end is None:
                fail(f"{rel}:{i + 1}: test-gated macro body is unbalanced")
                i = j + 1
                continue
            skip.update(range(i, end + 1))
            i = end + 1
            continue
        if ITEM_RE.match(item):
            # Test-only non-module item (e.g. a ledger const): not a cut point,
            # so production below it stays visible.
            i += 1
            continue
        if OPAQUE_RE.match(item):
            fail(
                f"{rel}:{i + 1}: test-gated attribute on an opaque body — "
                "cannot classify the item"
            )
            i += 1
            continue
        # A struct field, expression or statement cannot open a module span.
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
