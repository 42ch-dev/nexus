#!/usr/bin/env bash
# Prove the diff from REV (default HEAD) to the working tree changes comments only.
#   line check:  every removed line is a comment or blank line, every added line blank
#                (skipped with --tokens-only, e.g. for a rustfmt reflow commit);
#   token check: comment-blind Rust token streams and JS/TS ASTs are identical.
# Usage: diff_scope.sh [REV] [--tokens-only]
set -euo pipefail
rev=${1:-HEAD}; mode=${2:-}
here=$(cd "$(dirname "$0")" && pwd)
cd "$(git rev-parse --show-toplevel)"
tmp=$(mktemp -d); trap 'rm -rf "$tmp"' EXIT
[ "$mode" = --tokens-only ] || REV="$rev" PYTHONPATH="$here" PYTHONDONTWRITEBYTECODE=1 python3 - <<'PY'
import os, subprocess, sys
import fragments as F
files = subprocess.run(["git", "diff", "--name-only", "--diff-filter=M", os.environ["REV"]], capture_output=True, text=True, check=True).stdout.split()
bad = 0; removed = added = 0
for f in files:
    old = F.git("show", f"{os.environ['REV']}:{f}").splitlines()
    cls = F.classify(old, f)
    out = subprocess.run(["git", "diff", "-U0", os.environ["REV"], "--", f], capture_output=True, text=True, check=True).stdout
    for h in out.splitlines():
        if h.startswith("@@"):
            o = int(h.split()[1][1:].split(",")[0]) - 1
        elif h.startswith("-") and not h.startswith("---"):
            removed += 1
            if old[o].strip() and cls[o] is None:
                bad += 1; print("NON-COMMENT REMOVED", f, o + 1, old[o])
            o += 1
        elif h.startswith("+") and not h.startswith("+++"):
            added += 1
            if h[1:].strip():
                bad += 1; print("NON-BLANK ADDED", f, h)
print(f"line_check files={len(files)} removed={removed} added={added} violations={bad}")
sys.exit(1 if bad else 0)
PY
rs=(); js=()
while read -r f; do
  mkdir -p "$tmp/$(dirname "$f")"; git show "$rev:$f" > "$tmp/$f"
  case "$f" in *.rs) rs+=("$tmp/$f" "$f");; *.sh) bash -n "$f";; *) js+=("$tmp/$f" "$f");; esac
done < <(git diff --name-only --diff-filter=M "$rev")
CARGO_TARGET_DIR="${TMPDIR:-/tmp}/rstok-target" cargo run -q --release --manifest-path "$here/rstok/Cargo.toml" -- "${rs[@]}"
node "$here/tstok.mjs" "${js[@]}"
