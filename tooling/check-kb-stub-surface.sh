#!/usr/bin/env bash
# ---------------------------------------------------------------------------
# check-kb-stub-surface.sh — knowledge-entry seam structural guard (I-000590).
#
# The in-memory `KnowledgeEntryPort` stub is only meaningful while the conversion
# seam it is driven through keeps one owner per direction. This gate asserts,
# over tracked Rust sources:
#   1. exactly one definition of each conversion mapper, in the allowed owner
#      `crates/nexus-spoke-adapter/src/conversion/knowledge_entry.rs`;
#   2. that definition's declaration prefix is exactly `pub` — a
#      visibility-qualified copy (`pub(crate)`, `pub(super)`, `pub(in …)`) or a
#      bare / `unsafe` / `extern` declaration is a second surface and is
#      rejected;
#   3. no parallel `KnowledgePort` trait or impl beside `KnowledgeEntryPort`.
#
# Each file is read whole and declarations are matched with whitespace
# (newlines included) between tokens, so `pub(crate)` on one line with
# `fn name(` on the next is still one declaration, and a string literal
# containing `//` or `/*` cannot hide the code that follows it.
#
# Trade accepted: matching is textual, not a Rust parser, so declaration-looking
# text inside a comment or a string literal is reported as a definition (a
# commented-out `pub(crate) fn` fails the gate). That direction is deliberate —
# a silent pass is the failure this guard exists to prevent.
#
# This guard used to live in `tests/knowledge_entry_port_stub.rs`, which walked
# the checkout from `cargo test`: not deterministic outside the checkout,
# invisible as a CI gate, and blind to `pub(crate) fn` because it matched only
# `pub fn`.
#
# Usage (repository root):
#   bash tooling/check-kb-stub-surface.sh
#   bash tooling/check-kb-stub-surface.sh --selftest
#
# `SCAN_ROOT` overrides the tree under test (the self-test uses disposable
# fixtures); the allowed owner path stays relative to that root.
#
# `--selftest` asserts these fixture verdicts, all under a `mktemp -d`:
#   valid              → PASS
#   missing            → reject (one mapper has no definition)
#   duplicate          → reject (two definitions of one mapper)
#   url-string         → reject (`"http://…"` line, then `pub(crate) fn`)
#   block-string       → reject (`/*` inside a string, then `pub(crate) fn`)
#   split-visibility   → reject (`pub(crate)` and `fn mapper(` on separate lines)
#   commented-out      → reject (documented false-positive trade; must not crash)
#   parallel           → reject (parallel `KnowledgePort` trait + impl)
#
# Requires bash and perl (perl is used for whole-file matching).
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
# and `PORT <path> <line>` for each `KnowledgePort` trait/impl. `<prefix>` is the
# declaration text before `fn` with runs of whitespace collapsed (`pub`,
# `pub(crate)`, `unsafe`, or empty), so a split declaration still classifies.
scan() {
  # Both halves run from the scanned root: the file list stays relative.
  ( cd "$SCAN_ROOT" || exit 1
    if git rev-parse --git-dir >/dev/null 2>&1; then
      git ls-files -- 'crates/**/*.rs' 'apps/**/*.rs'
    else
      find crates apps -type f -name '*.rs' 2>/dev/null || true
    fi | sort > "$SCRATCH"

    perl - "$SCRATCH" "${MAPPERS[@]}" <<'PERL'
use strict;
use warnings;

my ($listfile, @names) = @ARGV;
open my $list, '<', $listfile or die "open $listfile: $!";
while (my $path = <$list>) {
  chomp $path;
  open my $fh, '<', $path or next;
  my $src = do { local $/; <$fh> };
  close $fh;

  for my $name (@names) {
    my $decl = qr/\b((?:pub(?:\s*\([^)]*\))?\s+)?
                    (?:(?:async|unsafe|const|extern(?:\s+"[^"]*")?)\s+)*)
                 fn\s+\Q$name\E\s*\(/x;
    while ($src =~ /$decl/g) {
      my $line = 1 + (substr($src, 0, $-[0]) =~ tr/\n//);
      my $prefix = $1;
      $prefix =~ s/\s+/ /g;
      $prefix =~ s/^ //;
      $prefix =~ s/ $//;
      print "MAPPER\t$name\t$path\t$line\t$prefix\n";
    }
  }

  my $port = qr/\b(?:trait\s+KnowledgePort|impl(?:<[^>]*>)?\s+KnowledgePort\s+for)\b/;
  while ($src =~ /$port/g) {
    my $line = 1 + (substr($src, 0, $-[0]) =~ tr/\n//);
    print "PORT\t$path\t$line\n";
  }
}
PERL
  )
}

check() {
  local hits rows count name path line pre failed=0
  command -v perl >/dev/null 2>&1 || fail "perl is required for whole-file matching"
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
      echo "      found prefix '${pre:-none}' — a visibility-qualified or bare definition is a second surface"
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
# unrelated `KnowledgeEntryPort` impl, and string literals that carry `//` and
# `/*` so a naive comment stripper would fail the clean case.
make_valid_fixture() {
  local dir="$1"
  mkdir -p "$dir/crates/nexus-spoke-adapter/src/conversion" "$dir/crates/consumer/src"
  cat > "$dir/$OWNER" <<'RS'
const DOC_URL: &str = "http://example";
const GLOB: &str = "crates/*/*.rs";

pub fn knowledge_record_to_spoke(entry: &Record) -> Wire {
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

  for case_dir in valid missing duplicate url-string block-string split-visibility commented-out parallel; do
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

  # url-string: a `//` inside a string must not hide the `pub(crate)` violation
  # that follows it.
  { echo 'const ENDPOINT: &str = "http://example/v1";'
    cat "$work/url-string/$OWNER"
  } | sed 's/^pub fn knowledge_record_to_spoke/pub(crate) fn knowledge_record_to_spoke/' \
    > "$work/url-string/$OWNER.tmp"
  mv "$work/url-string/$OWNER.tmp" "$work/url-string/$OWNER"

  # block-string: a `/*` inside a string must not open a comment that swallows
  # the `pub(crate)` violation after it.
  { echo 'const PATTERN: &str = "crates/*/*.rs";'
    cat "$work/block-string/$OWNER"
  } | sed 's/^pub fn knowledge_record_to_spoke/pub(crate) fn knowledge_record_to_spoke/' \
    > "$work/block-string/$OWNER.tmp"
  mv "$work/block-string/$OWNER.tmp" "$work/block-string/$OWNER"

  # split-visibility: `pub(crate)` and `fn mapper(` on separate lines.
  cat > "$work/split-visibility/$OWNER" <<'RS'
pub(crate)
fn knowledge_record_to_spoke(entry: &Record) -> Wire {
    Wire::from(entry)
}

pub fn spoke_to_knowledge_record(entry: Wire) -> Record {
    Record::from(entry)
}
RS

  # commented-out: reported as a definition (documented trade), never a crash.
  cat > "$work/commented-out/crates/consumer/src/extra.rs" <<'RS'
// pub(crate) fn knowledge_record_to_spoke(entry: &Record) -> Wire {
//     Wire::from(entry)
// }
RS

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
  expect duplicate 1 "found 2"
  expect url-string 1 "must be declared exactly \`pub fn\`"
  expect block-string 1 "must be declared exactly \`pub fn\`"
  expect split-visibility 1 "must be declared exactly \`pub fn\`"
  expect commented-out 1 "found 2"
  expect parallel 1 "parallel \`KnowledgePort\` trait/impl must not exist"

  if [ "$failures" -ne 0 ]; then
    rm -rf "$work"
    echo "FAIL: $failures self-test case(s) failed"
    exit 1
  fi
  rm -rf "$work"
  echo "PASS: self-test — clean tree accepted; missing, duplicate, string-masked, split-visibility, commented-out and parallel-trait fixtures rejected"
}

case "${1:-}" in
  "") check ;;
  --selftest) selftest ;;
  *) fail "unknown argument '$1' (expected none or --selftest)" ;;
esac
