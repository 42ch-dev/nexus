#!/usr/bin/env bash
# UI guardrails — wrapper drift & Design Studio boundary checks.
# Mirrors `.github/workflows/ci.yml` job `ui-guardrails`.
# Usage (from repository root): bash tooling/check-ui-guardrails.sh
#
# Implements:
#   .mstar/iterations/v1.100/specs/ui-guardrails-cn-ssot.md
#   § Guardrail implementation
#   § Promoted-wrapper forbidden imports
#   § Design Studio forbidden imports + transitional @web-ui/* annotation policy
#   § cn-parity test (R-V199QC1-S001): behavioral SSOT check (one authority for extendTailwindMerge; web re-exports)
#
# Precedent: tooling/check-schema-drift.sh (set -euo pipefail + grep + exit 1)

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

VIOLATIONS=0

# ── helper: check file(s) for forbidden pattern ──
# Usage: forbid "label" "pattern" file1 [file2 ...]
#   "pattern" is an egrep regex; use single quotes.
forbid() {
  local label="$1"; shift
  local pattern="$1"; shift
  local matches
  matches=$(grep -nE "$pattern" "$@" 2>/dev/null || true)
  if [ -n "$matches" ]; then
    echo "❌ ${label}"
    echo "$matches"
    VIOLATIONS=$((VIOLATIONS + 1))
  fi
}

# ── helper: check that every @web-ui/* import line in a file carries a transitional annotation ──
# Convention (per apps/design-studio/AGENTS.md): annotation must be on the SAME LINE as the import.
# "transitional" keyword is the canonical signal; both // @web-ui/<name> — transitional … and // transitional — … are accepted.
check_webui_annotations() {
  local file="$1"
  local webui_lines
  webui_lines=$(grep -nE "import\s+.*from\s+['\"]@web-ui/" "$file" 2>/dev/null || true)
  if [ -z "$webui_lines" ]; then
    return 0
  fi

  local any_unannotated=0
  while IFS= read -r line; do
    # line format: "<num>:<content>"
    # Accept both:
    #   // @web-ui/<name> — transitional …   (canonical per spec)
    #   // transitional — …                  (existing format, pre-T1)
    if ! echo "$line" | grep -qE 'transitional'; then
      echo "  ❌ ${line}"
      any_unannotated=1
    fi
  done <<< "$webui_lines"

  if [ "$any_unannotated" -eq 1 ]; then
    echo "❌ Studio: @web-ui/* imports missing transitional annotation in $file"
    # Print the offending lines again for clarity
    while IFS= read -r line; do
      if ! echo "$line" | grep -qE 'transitional'; then
        echo "  → $line"
      fi
    done <<< "$webui_lines"
    VIOLATIONS=$((VIOLATIONS + 1))
  fi
}

echo "==> Checking promoted-wrapper forbidden imports..."

# Wrapper file list — files that re-export from @42ch/nexus-ui.
# P1 wrappers (promoted): button, badge, card.
# P2 will promote input, label, textarea — add them here after promotion.
# A file is a "wrapper" iff it contains a re-export from @42ch/nexus-ui.
WRAPPER_DIR="apps/web/src/components/ui"
WRAPPER_CANDIDATES=(
  "$WRAPPER_DIR/button.tsx"
  "$WRAPPER_DIR/badge.tsx"
  "$WRAPPER_DIR/card.tsx"
  "$WRAPPER_DIR/input.tsx"
  "$WRAPPER_DIR/label.tsx"
  "$WRAPPER_DIR/textarea.tsx"
)

WRAPPER_EXISTING=()
for f in "${WRAPPER_CANDIDATES[@]}"; do
  if [ -f "$f" ] && grep -q "from '@42ch/nexus-ui'" "$f" 2>/dev/null; then
    WRAPPER_EXISTING+=("$f")
  fi
done

if [ ${#WRAPPER_EXISTING[@]} -eq 0 ]; then
  echo "⚠️  No confirmed wrapper files found — check is vacuously passing. Verify WRAPPER_DIR."
else
  echo "   Scanning ${#WRAPPER_EXISTING[@]} confirmed wrapper file(s): ${WRAPPER_EXISTING[*]}"

  # Forbidden patterns in promoted wrappers.
  # Matches ANY import/require of the forbidden dependency (not just top-level).
  # The public `export { … } from '@42ch/nexus-ui'` and `import type` are NOT matched.

  for pattern in \
    "import\s+.*\bclsx\b" \
    "import\s+.*\bclass-variance-authority\b" \
    "import\s+.*\btailwind-merge\b" \
    "import\s+.*from\s+['\"]@/lib/" \
    "import\s+.*from\s+['\"][^'\"]*\.\.\/lib/" \
    "import\s+.*from\s+['\"]@42ch/nexus-ui/src/"; do
    forbid "Wrapper imports forbidden dependency" "$pattern" "${WRAPPER_EXISTING[@]}"
  done
fi

echo ""
echo "==> Checking Design Studio forbidden imports..."

STUDIO_SRC="apps/design-studio/src"
STUDIO_FILES=()
while IFS= read -r -d '' f; do
  STUDIO_FILES+=("$f")
done < <(find "$STUDIO_SRC" \( -name '*.ts' -o -name '*.tsx' \) -print0 2>/dev/null || true)

if [ ${#STUDIO_FILES[@]} -eq 0 ]; then
  echo "⚠️  No Studio source files found at $STUDIO_SRC — check is vacuously passing."
else
  echo "   Scanning ${#STUDIO_FILES[@]} Studio source file(s)"

  # ── Forbidden import patterns in Design Studio ──

  # Web product pages (relative path from studio into web)
  forbid "Studio: imports web product pages" \
    "import\s+.*from\s+['\"][^'\"]*web\/src\/pages\/" \
    "${STUDIO_FILES[@]}"

  # Web layout shells
  forbid "Studio: imports web layout shells" \
    "import\s+.*from\s+['\"][^'\"]*web\/src\/components\/layout\/" \
    "${STUDIO_FILES[@]}"

  # Web daemon transport / client
  forbid "Studio: imports daemon client/transport" \
    "import\s+.*from\s+['\"][^'\"]*web\/src\/lib\/nexus\/" \
    "${STUDIO_FILES[@]}"

  # Web product hooks
  forbid "Studio: imports web product hooks" \
    "import\s+.*from\s+['\"][^'\"]*web\/src\/hooks\/" \
    "${STUDIO_FILES[@]}"

  # Web app providers / contexts
  forbid "Studio: imports web providers/contexts" \
    "import\s+.*from\s+['\"][^'\"]*web\/src\/(providers|contexts)\/" \
    "${STUDIO_FILES[@]}"

  # Wire contracts (studio is not an ACP consumer)
  forbid "Studio: imports @42ch/nexus-contracts" \
    "import\s+.*from\s+['\"]@42ch\/nexus-contracts['\"]" \
    "${STUDIO_FILES[@]}"

  # Deep import into @42ch/nexus-ui internals (must use public API)
  forbid "Studio: deep-imports @42ch/nexus-ui/src/*" \
    "import\s+.*from\s+['\"]@42ch\/nexus-ui\/src\/" \
    "${STUDIO_FILES[@]}"

  # Tauri helpers (studio is browser-only SPA)
  forbid "Studio: imports @tauri-apps/*" \
    "import\s+.*from\s+['\"]@tauri-apps\/" \
    "${STUDIO_FILES[@]}"

  # ── @web-ui/* annotation enforcement ──
  echo ""
  echo "   Checking @web-ui/* transitional annotations..."
  for f in "${STUDIO_FILES[@]}"; do
    check_webui_annotations "$f"
  done

  # ── @web-ui/* for already-promoted primitives (Button, Badge, Card) ──
  # These must be imported from @42ch/nexus-ui, not @web-ui/*
  echo ""
  echo "   Checking @web-ui/* for already-promoted primitives..."
  for promoted in button badge card input label textarea; do
    matches=$(grep -nE "import\s+.*from\s+['\"]@web-ui/$promoted['\"]" "${STUDIO_FILES[@]}" 2>/dev/null || true)
    if [ -n "$matches" ]; then
      echo "❌ Studio: imports already-promoted primitive @web-ui/$promoted (use @42ch/nexus-ui)"
      echo "$matches"
      VIOLATIONS=$((VIOLATIONS + 1))
    fi
  done
fi

echo ""
echo "==> Checking cn consolidation (package ↔ web)..."

check_cn_parity() {
  local pkg_cn="packages/nexus-ui/src/lib/cn.ts"
  local pkg_barrel="packages/nexus-ui/src/index.ts"
  local web_cn="apps/web/src/lib/utils.ts"

  # ── 1. Package cn.ts must exist and own the extendTailwindMerge config ──
  if [ ! -f "$pkg_cn" ]; then
    echo "❌ cn consolidation: missing $pkg_cn"
    VIOLATIONS=$((VIOLATIONS + 1))
    return
  fi
  if ! grep -q 'extendTailwindMerge' "$pkg_cn" 2>/dev/null; then
    echo "❌ cn consolidation: $pkg_cn does not contain extendTailwindMerge (expected SSOT)"
    VIOLATIONS=$((VIOLATIONS + 1))
    return
  fi
  echo "   ✅ $pkg_cn owns extendTailwindMerge (SSOT)."

  # ── 2. Package barrel must export cn ──
  if ! grep -qE "export\s+\{\s*cn\s*\}" "$pkg_barrel" 2>/dev/null; then
    echo "❌ cn consolidation: $pkg_barrel does not export cn"
    VIOLATIONS=$((VIOLATIONS + 1))
    return
  fi
  echo "   ✅ $pkg_barrel exports cn."

  # ── 3. Web utils.ts must be a thin re-export (no local implementation) ──
  if [ ! -f "$web_cn" ]; then
    echo "❌ cn consolidation: missing $web_cn"
    VIOLATIONS=$((VIOLATIONS + 1))
    return
  fi

  # Re-export check: must import/re-export cn from @42ch/nexus-ui
  if ! grep -qE "export\s+\{\s*cn\s*\}\s*from\s+['\"]@42ch/nexus-ui['\"]" "$web_cn" 2>/dev/null; then
    echo "❌ cn consolidation: $web_cn must re-export cn from @42ch/nexus-ui (e.g. 'export { cn } from \"@42ch/nexus-ui\"')"
    VIOLATIONS=$((VIOLATIONS + 1))
  else
    echo "   ✅ $web_cn re-exports cn from @42ch/nexus-ui."
  fi

  # Must NOT contain a local extendTailwindMerge implementation.
  # Check for actual import/require of tailwind-merge (not just the word in comments).
  if grep -qE "(import|require).*tailwind-merge" "$web_cn" 2>/dev/null; then
    echo "❌ cn consolidation: $web_cn imports tailwind-merge locally (must delegate to @42ch/nexus-ui)"
    VIOLATIONS=$((VIOLATIONS + 1))
  else
    echo "   ✅ $web_cn has no local tailwind-merge import."
  fi

  # ── 4. No other file duplicates extendTailwindMerge config ──
  # Search for duplicate extendTailwindMerge across the repo (exclude the SSOT file).
  local dupes
  dupes=$(grep -rl 'extendTailwindMerge' apps/ --include='*.ts' --include='*.tsx' 2>/dev/null | grep -v 'node_modules' | grep -v 'packages/nexus-ui/src/lib/cn.ts' || true)
  if [ -n "$dupes" ]; then
    for d in $dupes; do
      # Files that are re-exports with just 'export { cn } from...' are fine
      if grep -qE "from\s+['\"]@42ch/nexus-ui['\"]" "$d" 2>/dev/null && ! grep -qE "import.*extendTailwindMerge" "$d" 2>/dev/null; then
        continue
      fi
      # Any other file with extendTailwindMerge is a duplicate
      if grep -qE "import.*extendTailwindMerge|require.*extendTailwindMerge|from.*tailwind-merge" "$d" 2>/dev/null; then
        echo "❌ cn consolidation: duplicate extendTailwindMerge config in $d (authority is $pkg_cn)"
        VIOLATIONS=$((VIOLATIONS + 1))
      fi
    done
  fi
  if [ "${dupes:-}" = "" ] || [ "$VIOLATIONS" -eq 0 ]; then
    # Re-check: only flag increase means a dupe was found; avoid noisy "pass" when no-dupes but other violations exist.
    : # pass — no new violations from dupe scan
  fi
}

check_cn_parity

echo ""
echo ""
# ── Summary ──
if [ "$VIOLATIONS" -eq 0 ]; then
  echo "✅ All UI guardrail checks passed."
  exit 0
else
  echo "❌ ${VIOLATIONS} UI guardrail violation(s) found. See details above."
  exit 1
fi
