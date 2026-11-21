#!/usr/bin/env bash
# ---------------------------------------------------------------------------
# check-kb-stub-surface.sh — knowledge-entry seam structural guard (I-000590).
#
# The in-memory `KnowledgeEntryPort` stub is only meaningful while the conversion
# seam it is driven through keeps one owner per direction. This gate asserts,
# over tracked Rust sources:
#   1. exactly one definition of each conversion mapper, in the allowed owner
#      `crates/nexus-spoke-adapter/src/conversion/knowledge_entry.rs` and declared
#      exactly `pub fn`;
#   2. no additional visibility-qualified definition (`pub(crate)`, `pub(super)`,
#      `pub(in …)`, or bare `fn`) of those mappers anywhere else;
#   3. no parallel `KnowledgePort` trait or impl beside `KnowledgeEntryPort`.
#
# This used to live in `tests/knowledge_entry_port_stub.rs`, which walked the
# checkout from `cargo test`: not deterministic outside the checkout, invisible
# as a CI gate, and blind to `pub(crate) fn` because it matched only `pub fn`.
# Comment content is stripped before matching, and declarations are matched as
# whole words, so prose or a longer identifier cannot satisfy a needle.
#
# Usage (repository root):
#   bash tooling/check-kb-stub-surface.sh
#   bash tooling/check-kb-stub-surface.sh --selftest
#
# `SCAN_ROOT` overrides the tree under test (the self-test uses disposable
# fixtures); the allowed owner path stays relative to that root.
#
# Exit codes:
#   0 — surface matches: one owner per mapper, no parallel port
#   1 — mismatch, missing source tree, or self-test failure
# ---------------------------------------------------------------------------
set -euo pipefail

SELF="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/$(basename "${BASH_SOURCE[0]}")"
ROOT="$(cd "$(dirname "$SELF")/.." && pwd)"
SCAN_ROOT="${SCAN_ROOT:-$ROOT}"

OWNER="crates/nexus-spoke-adapter/src/conversion/knowledge_entry.rs"
MAPPERS=(knowledge_record_to_spoke spoke_to_knowledge_record)

fail() {
  echo "FAIL: $1"
  exit 1
}

# Emits `MAPPER <name> <path> <line> <prefix>` for each declaration of a mapper
# and `PORT <path> <line>` for each `KnowledgePort` trait/impl. The scan works on
# comment-stripped text; rustdoc examples and prose stay out of the count. Each
# file is read on its own so an unterminated block comment cannot swallow the
# files after it.
# shortcut: stripping is textual, so `//` or `/*` inside a string literal hides
# the rest of that line — upgrade to a parser if a needle ever sits beside one.
scan() {
  # Both halves run from the scanned root: the file list stays relative.
  ( cd "$SCAN_ROOT" || exit 1
    if git rev-parse --git-dir >/dev/null 2>&1; then
      git ls-files -- 'crates/**/*.rs' 'apps/**/*.rs'
    else
      find crates apps -type f -name '*.rs' 2>/dev/null || true
    fi | sort > "$SCRATCH"
    awk -v list="$SCRATCH" -v names="${MAPPERS[*]}" '
    function strip(line,   out, i, n, c, d) {
      if (!inblock && line !~ /\/\*|\/\//) return line
      out = ""; i = 1; n = length(line)
      while (i <= n) {
        c = substr(line, i, 1); d = substr(line, i, 2)
        if (inblock) {
          if (d == "*/") { inblock = 0; i += 2 } else { i++ }
          continue
        }
        if (d == "/*") { inblock = 1; i += 2; continue }
        if (d == "//") break
        out = out c; i++
      }
      return out
    }
    BEGIN {
      count = split(names, needles, " ")
      while ((getline path < list) > 0) {
        inblock = 0; line_no = 0
        while ((getline line < path) > 0) {
          line_no++
          text = strip(line)
          for (k = 1; k <= count; k++) {
            name = needles[k]
            if (match(text, "(^|[^[:alnum:]_])fn[[:space:]]+" name "[[:space:]]*\\(")) {
              pre = substr(text, 1, RSTART - 1)
              gsub(/^[[:space:]]+|[[:space:]]+$/, "", pre)
              printf "MAPPER\t%s\t%s\t%d\t%s\n", name, path, line_no, pre
            }
          }
          if (match(text, "(^|[^[:alnum:]_])trait[[:space:]]+KnowledgePort([^[:alnum:]_]|$)") ||
              match(text, "(^|[^[:alnum:]_])impl([[:space:]]*<[^>]*>)?[[:space:]]+KnowledgePort[[:space:]]+for([^[:alnum:]_]|$)"))
            printf "PORT\t%s\t%d\n", path, line_no
        }
        close(path)
      }
    }'
  )
}

check() {
  local hits rows count name path line pre failed=0
  [ -d "$SCAN_ROOT/crates" ] || fail "no crates/ directory under $SCAN_ROOT"
  SCRATCH="$(mktemp "${TMPDIR:-/tmp}/kb-stub-surface.XXXXXX")"
  trap 'rm -f "$SCRATCH"' EXIT
  hits="$(scan)"

  for name in "${MAPPERS[@]}"; do
    rows="$(printf '%s\n' "$hits" | awk -F'\t' -v want="$name" '$1 == "MAPPER" && $2 == want')"
    count="$(printf '%s\n' "$rows" | grep -c '[^[:space:]]' || true)"
    if [ "$count" -ne 1 ]; then
      echo "FAIL: expected exactly one definition of \`fn $name\` under crates/ and apps/, found $count:"
      printf '%s\n' "$rows" | awk -F'\t' 'NF == 5 { printf "  %s:%s (prefix %s)\n", $3, $4, ($5 == "" ? "none" : $5) }'
      failed=1
      continue
    fi
    IFS=$'\t' read -r _ _ path line pre <<<"$rows"
    if [ "$path" != "$OWNER" ]; then
      echo "FAIL: \`fn $name\` belongs in $OWNER, found a definition at $path:$line"
      failed=1
    elif [ "$pre" != "pub" ]; then
      echo "FAIL: \`fn $name\` at $path:$line must be declared exactly \`pub fn\`;"
      echo "      found prefix '$pre' — an extra visibility-qualified definition is a second surface"
      failed=1
    else
      echo "OK: one \`pub fn $name\` in $OWNER"
    fi
  done

  rows="$(printf '%s\n' "$hits" | awk -F'\t' '$1 == "PORT"')"
  if [ -n "$(printf '%s' "$rows" | tr -d '[:space:]')" ]; then
    echo "FAIL: a parallel \`KnowledgePort\` trait/impl must not exist; \`KnowledgeEntryPort\` is the only knowledge port:"
    printf '%s\n' "$rows" | sed 's/^/  /'
    failed=1
  else
    echo "OK: no parallel \`KnowledgePort\` trait/impl"
  fi

  [ "$failed" -eq 0 ] || { rm -f "$SCRATCH"; exit 1; }
  rm -f "$SCRATCH"
  echo "PASS: knowledge-entry seam surface is intact"
}

# Writes a minimal valid fixture tree: both mappers in the allowed owner, an
# unrelated `KnowledgeEntryPort` impl, and a comment that names a mapper.
make_valid_fixture() {
  local dir="$1"
  mkdir -p "$dir/crates/nexus-spoke-adapter/src/conversion" "$dir/crates/consumer/src"
  cat > "$dir/$OWNER" <<'RS'
pub fn knowledge_record_to_spoke(entry: &Record) -> Wire {
    // `pub fn knowledge_record_to_spoke(` in a comment must not count.
    Wire::from(entry)
}

pub fn spoke_to_knowledge_record(entry: Wire) -> Record {
    Record::from(entry)
}
RS
  cat > "$dir/crates/consumer/src/lib.rs" <<'RS'
impl KnowledgeEntryPort for Consumer {
    fn put(&self) {}
}
RS
}

selftest() {
  local work case_dir out status failures=0
  work="$(mktemp -d)"
  trap 'rm -rf "$work"' EXIT

  expect() {
    local label="$1" want_exit="$2" want_text="$3" dir="$work/$1" summary
    shift 3
    status=0
    out="$(SCAN_ROOT="$dir" bash "$SELF" 2>&1)" || status=$?
    if [ "$status" -ne "$want_exit" ]; then
      echo "FAIL: self-test '$label' exited $status, expected $want_exit"
      echo "$out" | sed 's/^/  /'
      failures=$((failures + 1))
    elif [ -n "$want_text" ] && ! printf '%s' "$out" | grep -qF -- "$want_text"; then
      echo "FAIL: self-test '$label' did not report '$want_text'"
      echo "$out" | sed 's/^/  /'
      failures=$((failures + 1))
    else
      summary="$(printf '%s\n' "$out" | grep -m1 '^FAIL' || true)"
      echo "OK: self-test '$label' -> ${summary:-accepted}"
    fi
  }

  for case_dir in valid missing duplicate visibility parallel; do
    make_valid_fixture "$work/$case_dir"
  done

  # missing: one direction has no definition at all.
  sed '/^pub fn spoke_to_knowledge_record/,/^}/d' \
    "$work/missing/$OWNER" > "$work/missing/$OWNER.tmp"
  mv "$work/missing/$OWNER.tmp" "$work/missing/$OWNER"

  # duplicate: a second copy of the same mapper elsewhere.
  cat > "$work/duplicate/crates/consumer/src/extra.rs" <<'RS'
pub fn knowledge_record_to_spoke(entry: &Record) -> Wire {
    Wire::from(entry)
}
RS

  # visibility: the owner declaration is `pub(crate)` instead of `pub`.
  sed 's/^pub fn knowledge_record_to_spoke/pub(crate) fn knowledge_record_to_spoke/' \
    "$work/visibility/$OWNER" > "$work/visibility/$OWNER.tmp"
  mv "$work/visibility/$OWNER.tmp" "$work/visibility/$OWNER"

  # parallel: a second knowledge port trait and its impl.
  cat > "$work/parallel/crates/consumer/src/extra.rs" <<'RS'
pub trait KnowledgePort {
    fn put(&self);
}

impl KnowledgePort for Consumer {
    fn put(&self) {}
}
RS

  expect valid 0 "PASS: knowledge-entry seam surface is intact"
  expect missing 1 "expected exactly one definition of \`fn spoke_to_knowledge_record\`"
  expect duplicate 1 "expected exactly one definition of \`fn knowledge_record_to_spoke\`"
  expect visibility 1 "must be declared exactly \`pub fn\`"
  expect parallel 1 "parallel \`KnowledgePort\` trait/impl must not exist"

  if [ "$failures" -ne 0 ]; then
    rm -rf "$work"
    echo "FAIL: $failures self-test case(s) failed"
    exit 1
  fi
  rm -rf "$work"
  echo "PASS: self-test — valid fixture accepted; missing, duplicate, visibility and parallel-trait fixtures rejected"
}

case "${1:-}" in
  "") check ;;
  --selftest) selftest ;;
  *) fail "unknown argument '$1' (expected none or --selftest)" ;;
esac
