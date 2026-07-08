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
    "import\s+.*from\s+['\"]\.\.\/lib/" \
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
  for promoted in button badge card; do
    matches=$(grep -nE "import\s+.*from\s+['\"]@web-ui/$promoted['\"]" "${STUDIO_FILES[@]}" 2>/dev/null || true)
    if [ -n "$matches" ]; then
      echo "❌ Studio: imports already-promoted primitive @web-ui/$promoted (use @42ch/nexus-ui)"
      echo "$matches"
      VIOLATIONS=$((VIOLATIONS + 1))
    fi
  done
fi

echo ""

# ── Summary ──
if [ "$VIOLATIONS" -eq 0 ]; then
  echo "✅ All UI guardrail checks passed."
  exit 0
else
  echo "❌ ${VIOLATIONS} UI guardrail violation(s) found. See details above."
  exit 1
fi
