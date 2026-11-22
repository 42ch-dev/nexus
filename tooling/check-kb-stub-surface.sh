#!/usr/bin/env bash
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
# A single lexer pass per file tracks code, `//` line comments, `/* … */` block
# comments (nested), `"` / raw strings and char literals, and emits a code
# projection of the same length (non-code characters become spaces, newlines
# kept). Declarations are matched on that projection with whitespace — newlines
# included — between tokens, so:
#   - a declaration-looking comment or string literal is neither counted nor
#     reported;
#   - `pub(crate)` on one line with `fn name(` on the next is still one
#     declaration;
#   - a comment or string cannot hide the code after it, because no code
#     character is ever removed from the projection.
#
# This guard used to live in `tests/knowledge_entry_port_stub.rs`, which walked
# the checkout from `cargo test`: not deterministic outside the checkout,
# invisible as a CI gate, and blind to `pub(crate) fn` because it matched only
# `pub fn`.
#
# The scan covers exactly the tracked set (`git ls-files` under `crates/` and
# `apps/`); a tracked file that cannot be opened or read aborts the run with the
# path and the error — it is never skipped, because skipping is how a violation
# in that file would pass.
#
# Known limit: this is a declaration-shaped regex over a code projection, not a
# Rust parser, so a mapper whose only definition comes from a `macro_rules!`
# expansion is invisible to it — expansion happens after this guard reads the
# source.
#
# Usage (repository root):
#   bash tooling/check-kb-stub-surface.sh
#   bash tooling/check-kb-stub-surface.sh --selftest
#
# `SCAN_ROOT` overrides the tree under test (the self-test uses disposable
# fixtures); the allowed owner path stays relative to that root.
#
# `--selftest` asserts these fixture verdicts, all under a `mktemp -d`:
#   valid              → PASS (a comment and a string name a mapper; ignored)
#   missing            → reject, `found 0` (one mapper has no definition)
#   duplicate          → reject, `found 2` (two definitions of one mapper)
#   line-comment       → reject, visibility (a `//` line, then a genuine
#                        `pub(crate) fn` on the following line)
#   block-string       → reject, visibility (`/*` inside a string, then a
#                        `pub(crate) fn`)
#   split-visibility   → reject, visibility (`pub(crate)` and `fn mapper(` on
#                        separate lines)
#   generic-duplicate  → reject, `found 2` (a generic declaration with a nested
#                        bound, plus the plain owner declaration)
#   raw-duplicate      → reject, `found 2` (the same mapper declared under the
#                        raw `r#` spelling in another crate)
#   comment-only       → reject, `found 0` (real declaration deleted; a comment
#                        holds the only declaration-shaped text)
#   string-only        → reject, `found 0` (same, text inside a string literal)
#   parallel           → reject (parallel `KnowledgePort` trait + impl)
#
# Requires bash and perl (perl runs the lexer and whole-file matching).
#
# Exit codes:
#   0 — surface matches: one owner per mapper, no parallel port
#   1 — mismatch, missing source tree, or self-test failure
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
  # Tracked mode is decided by the checkout itself: asking `git` would let a
  # broken `git` fall back to a filesystem walk over a partial tree.
  ( cd "$SCAN_ROOT" || exit 1
    if [ -e .git ]; then
      if ! git ls-files -- 'crates/**/*.rs' 'apps/**/*.rs' > "$SCRATCH"; then
        echo "FAIL: git ls-files failed while listing tracked sources under $SCAN_ROOT" >&2
        exit 1
      fi
    else
      # Fixture trees need not carry an `apps/` root.
      if ! find crates -type f -name '*.rs' > "$SCRATCH"; then
        echo "FAIL: find failed while listing sources under $SCAN_ROOT/crates" >&2
        exit 1
      fi
      if [ -d apps ]; then
        if ! find apps -type f -name '*.rs' >> "$SCRATCH"; then
          echo "FAIL: find failed while listing sources under $SCAN_ROOT/apps" >&2
          exit 1
        fi
      fi
    fi

    # Zero files means the root or pathspec is wrong, never "clean".
    if [ ! -s "$SCRATCH" ]; then
      echo "FAIL: no .rs files found under crates/ or apps/ in $SCAN_ROOT — wrong root or pathspec" >&2
      exit 1
    fi

    perl - "$SCRATCH" "${MAPPERS[@]}" <<'PERL'
use strict;
use warnings;

# Returns the source with every non-code character replaced by a space, keeping
# newlines so byte offsets and line numbers survive. Code characters are copied
# verbatim, so this can never hide a declaration that is real code.
sub project_code {
  my ($src) = @_;
  my $n = length $src;
  my $out = '';
  my $i = 0;

  while ($i < $n) {
    pos($src) = $i;
    if ($src =~ /\G([^\/"']+)/gc) {
      $out .= $1;
      $i = pos($src);
      next;
    }

    my $c = substr($src, $i, 1);
    my $two = substr($src, $i, 2);

    # `//` to end of line; the newline itself stays code-handled next.
    if ($two eq '//') {
      my $end = index($src, "\n", $i);
      $end = $n if $end < 0;
      $out .= ' ' x ($end - $i);
      $i = $end;
      next;
    }

    # `/* ... */` with nesting depth, as Rust allows.
    if ($two eq '/*') {
      my ($depth, $j) = (0, $i);
      while ($j < $n) {
        my $d = substr($src, $j, 2);
        if ($d eq '/*') { $depth++; $j += 2; next; }
        if ($d eq '*/') { $depth--; $j += 2; last if $depth == 0; next; }
        $j++;
      }
      my $chunk = substr($src, $i, $j - $i);
      $chunk =~ tr/\n/ /c;
      $out .= $chunk;
      $i = $j;
      next;
    }

    # `"..."` or `r"..."` / `r#"..."#` (also `br#"…"#`, `cr#"…"#`).
    if ($c eq '"') {
      my $hashes = 0;
      $hashes++ while $i - $hashes - 1 >= 0 && substr($src, $i - $hashes - 1, 1) eq '#';
      my $raw = $i - $hashes - 1 >= 0 && substr($src, $i - $hashes - 1, 1) eq 'r';
      my $j = $i + 1;
      if ($raw) {
        my $term = '"' . ('#' x $hashes);
        my $k = index($src, $term, $j);
        $j = $k < 0 ? $n : $k + length($term);
      } else {
        while ($j < $n) {
          my $ch = substr($src, $j, 1);
          if ($ch eq '\\') { $j += 2; next; }
          $j++;
          last if $ch eq '"';
        }
        $j = $n if $j > $n;
      }
      my $chunk = substr($src, $i, $j - $i);
      $chunk =~ tr/\n/ /c;
      $out .= $chunk;
      $i = $j;
      next;
    }

    # `'x'`, `'\n'`, `'\u{…}'` are literals; `'a` is a lifetime and stays code.
    if ($c eq "'") {
      my $next = substr($src, $i + 1, 1);
      my $j;
      if ($next eq '\\') {
        my $k = index($src, "'", $i + 2);
        $j = $k < 0 ? $i + 1 : $k + 1;
      } elsif ($next ne '' && substr($src, $i + 2, 1) eq "'") {
        $j = $i + 3;
      }
      if (defined $j) {
        my $chunk = substr($src, $i, $j - $i);
        $chunk =~ tr/\n/ /c;
        $out .= $chunk;
        $i = $j;
        next;
      }
    }

    $out .= $c;
    $i++;
  }
  return $out;
}

my ($listfile, @names) = @ARGV;
open my $list, '<', $listfile or die "open $listfile: $!";
while (my $path = <$list>) {
  chomp $path;
  open my $fh, '<', $path or die "FAIL: cannot read $path: $!\n";
  my $src = '';
  while (1) {
    my $read = sysread $fh, my $buf, 1 << 16;
    die "FAIL: read failed for $path: $!\n" unless defined $read;
    last if $read == 0;
    $src .= $buf;
  }
  close $fh or die "FAIL: read failed for $path: $!\n";
  my $code = project_code($src);

  for my $name (@names) {
    # `[<(]` accepts a generic parameter list without parsing its nesting
    # (`fn f<T: Into<U>>`): the token after the name is `<` or `(` exactly when
    # this is a declaration, so a generic definition cannot hide from the count.
    # `(?:r\#)?` treats the raw spelling as the same Rust identifier (`#` is
    # escaped because /x would otherwise start a comment).
    my $decl = qr/\b((?:pub(?:\s*\([^)]*\))?\s+)?
                    (?:(?:async|unsafe|const|extern(?:\s+"[^"]*")?)\s+)*)
                 fn\s+(?:r\#)?\Q$name\E\s*[<(]/x;
    while ($code =~ /$decl/g) {
      my $line = 1 + (substr($src, 0, $-[0]) =~ tr/\n//);
      my $prefix = $1;
      $prefix =~ s/\s+/ /g;
      $prefix =~ s/^ //;
      $prefix =~ s/ $//;
      print "MAPPER\t$name\t$path\t$line\t$prefix\n";
    }
  }

  my $port = qr/\b(?:trait\s+KnowledgePort|impl(?:<[^>]*>)?\s+KnowledgePort\s+for)\b/;
  while ($code =~ /$port/g) {
    my $line = 1 + (substr($src, 0, $-[0]) =~ tr/\n//);
    print "PORT\t$path\t$line\n";
  }
}
PERL
  )
}

check() {
  local hits rows count name path line pre failed=0
  command -v perl >/dev/null 2>&1 || fail "perl is required for the code-state lexer"
  [ -d "$SCAN_ROOT/crates" ] || fail "no crates/ directory under $SCAN_ROOT"
  SCRATCH="$(mktemp "${TMPDIR:-/tmp}/kb-stub-surface.XXXXXX")"
  trap 'rm -f "$SCRATCH"' EXIT
  # A scan failure (unreadable tracked file, failed listing) aborts; it never
  # degrades to a partial corpus that could pass.
  hits="$(scan)" || { rm -f "$SCRATCH"; exit 1; }

  for name in "${MAPPERS[@]}"; do
    rows="$(printf '%s\n' "$hits" | awk -F'\t' -v want="$name" '$1 == "MAPPER" && $2 == want')"
    count="$(printf '%s\n' "$rows" | awk 'NF && $0 !~ /^[[:space:]]*$/ { n++ } END { print n + 0 }')"
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
# unrelated `KnowledgeEntryPort` impl, and a comment plus string literals that
# name a mapper, so a scan that counts non-code text fails the clean case.
make_valid_fixture() {
  local dir="$1"
  mkdir -p "$dir/crates/nexus-spoke-adapter/src/conversion" "$dir/crates/consumer/src"
  cat > "$dir/$OWNER" <<'RS'
const DOC_URL: &str = "http://example";
const GLOB: &str = "crates/*/*.rs";
const DOC: &str = "pub fn knowledge_record_to_spoke(entry) -> Wire";

/// The seam is `pub fn knowledge_record_to_spoke(` plus its reverse.
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
  trap 'rm -rf "${work:-}"' EXIT

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
      summary="$(printf '%s\n' "$out" | awk '/^FAIL/ { print; exit }')"
      echo "OK: self-test '$label' -> ${summary:-accepted}"
    fi
  }

  for case_dir in valid missing duplicate line-comment block-string split-visibility generic-duplicate raw-duplicate comment-only string-only parallel; do
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

  # line-comment: a `//` line must not swallow the genuine `pub(crate)`
  # declaration on the following line.
  { echo '// The mapper lives on the next line; this comment must not hide it.'
    cat "$work/line-comment/$OWNER"
  } | sed 's/^pub fn knowledge_record_to_spoke/pub(crate) fn knowledge_record_to_spoke/' \
    > "$work/line-comment/$OWNER.tmp"
  mv "$work/line-comment/$OWNER.tmp" "$work/line-comment/$OWNER"

  # block-string: a `/*` inside a string must not open a comment that swallows
  # the `pub(crate)` declaration after it.
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

  # generic-duplicate: a generic declaration whose bound nests `>` plus the
  # plain owner declaration is two definitions of the same mapper.
  cat > "$work/generic-duplicate/crates/consumer/src/extra.rs" <<'RS'
pub fn knowledge_record_to_spoke<T: Into<Record>>(entry: T) -> Wire {
    Wire::from(entry.into())
}
RS

  # raw-duplicate: `r#name` and `name` are the same identifier, so a foreign
  # declaration under the raw spelling is a second definition.
  cat > "$work/raw-duplicate/crates/consumer/src/extra.rs" <<'RS'
pub fn r#knowledge_record_to_spoke(entry: &Record) -> Wire {
    Wire::from(entry)
}
RS

  # comment-only: the real declaration is gone; a comment holds the only
  # declaration-shaped text, which must not count as the mapper.
  sed '/^\/\/\/ The seam/,/^}/d' \
    "$work/comment-only/$OWNER" > "$work/comment-only/$OWNER.tmp"
  mv "$work/comment-only/$OWNER.tmp" "$work/comment-only/$OWNER"
  cat > "$work/comment-only/crates/consumer/src/extra.rs" <<'RS'
// pub fn knowledge_record_to_spoke(entry: &Record) -> Wire {
//     Wire::from(entry)
// }
RS

  # string-only: same, but the declaration-shaped text sits in a string.
  sed '/^\/\/\/ The seam/,/^}/d' \
    "$work/string-only/$OWNER" > "$work/string-only/$OWNER.tmp"
  mv "$work/string-only/$OWNER.tmp" "$work/string-only/$OWNER"
  cat > "$work/string-only/crates/consumer/src/extra.rs" <<'RS'
const SIGNATURE: &str = "pub fn knowledge_record_to_spoke(entry: &Record) -> Wire {";
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
  expect missing 1 "found 0"
  expect duplicate 1 "found 2"
  expect line-comment 1 "must be declared exactly \`pub fn\`"
  expect block-string 1 "must be declared exactly \`pub fn\`"
  expect split-visibility 1 "must be declared exactly \`pub fn\`"
  expect generic-duplicate 1 "found 2"
  expect raw-duplicate 1 "found 2"
  expect comment-only 1 "found 0"
  expect string-only 1 "found 0"
  expect parallel 1 "parallel \`KnowledgePort\` trait/impl must not exist"

  if [ "$failures" -ne 0 ]; then
    rm -rf "$work"
    echo "FAIL: $failures self-test case(s) failed"
    exit 1
  fi
  rm -rf "$work"
  echo "PASS: self-test — clean tree accepted; missing, duplicate, visibility, generic-duplicate, raw-identifier duplicate, comment-only, string-only and parallel-trait fixtures rejected"
}

case "${1:-}" in
  "") check ;;
  --selftest) selftest ;;
  *) fail "unknown argument '$1' (expected none or --selftest)" ;;
esac
