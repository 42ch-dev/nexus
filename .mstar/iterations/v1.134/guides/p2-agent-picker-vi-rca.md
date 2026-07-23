# P2 AgentPicker VI — RCA (T1)

**plan_id:** `2026-07-23-v1.134-p2-agent-picker-vi-retune`  
**worktree:** `plan/v1.134-p2-agent-picker-vi-retune`  
**captured:** 2026-07-23  
**SSOT file:** `apps/web/src/components/setup/agent-picker.tsx`

## Executive summary

| Field | Value |
|-------|-------|
| **Status** | Recovered |
| **Removal commit** | `d9d7a41c` — V1.132 P2 T3 (`feat(vi): theme-split primary Button and Setup selection ring`) in PR #170 |
| **Last good parent** | `d9d7a41c^` (`12eb8a21`, VI Chronos confirm #167) — `StatusDot` present through V1.119 |
| **Correction** | Dots were **not** removed during V1.108–V1.119 polish; they were **refined** there (FB-UI-006) and **deleted** only in V1.132 P2 |
| **Component removed** | `StatusDot` — one top-right circular affordance per agent card (serves both “selection status dot” and “top-right status dot” in compass language) |
| **Author intent** | “圆点都没了”; “右上角的状态圆点别删” — restore the dot; Light shell cyan = accent-only (no washes) |

**One-line dot spec:** Restore `StatusDot` at the top-right of each card identity row — 10×10px circle: **lit** = `bg-green-700` when installed+selected; **hollow** = `border-[1.5px] border-gray-500` when installed+unselected; **muted** = `bg-gray-500` when not installed — paired with a **2px `border-blue-700` selection ring** on the card (no cyan fill wash in Light).

---

## Timeline (git evidence)

| Commit | PR / tag | `StatusDot` state | Card selection chrome |
|--------|----------|-------------------|------------------------|
| `c976aae8` | V1.102 #132 | **Added** — hollow green unselected; lit green + `ring-2 ring-blue-700` on dot when selected | `border-blue-700 bg-blue-700/8` when selected |
| `715f85ca` | V1.108 #138 | **FB-UI-006** — unselected hollow **gray** (`border-gray-500`); selected filled green only (ring removed from dot) | Whole-card hover (FB-UI-007); selected `border-blue-700 bg-blue-700/8` |
| `da1fd3e9` | V1.110 #140 | Unchanged | Unchanged |
| `dfa3ecf8` | V1.119 #151 | Unchanged | Unchanged |
| `12eb8a21` | VI Chronos #167 | Unchanged (link colors only) | Unchanged |
| `d9d7a41c` | V1.132 #170 P2 T3 | **Deleted** — `StatusDot` + `agent-status-dot` testid removed | Replaced with `border-2 border-blue-700` ring only; comment at line 15 documents “no … right-side status dot” |

### Removal diff (authoritative)

Commit `d9d7a41c` removed:

1. `<StatusDot installed={agent.installed} selected={selected} />` from `AgentCardIdentity`
2. The entire `StatusDot` function (~45 lines)
3. `selected` prop threading into `AgentCardIdentity`
4. Selected-card cyan wash (`bg-blue-700/8`) in favor of `border-2 border-blue-700`

Pre-removal `StatusDot` markup (parent `d9d7a41c^`):

```tsx
function StatusDot({ installed, selected }: { installed: boolean; selected: boolean }) {
  const { t } = useTranslation('setup');
  const label = installed
    ? selected ? t('agentPicker.status.selected') : t('agentPicker.status.installed')
    : t('agentPicker.status.notInstalled');

  return (
    <span
      className="relative mt-0.5 inline-flex h-2.5 w-2.5 shrink-0"
      title={label}
      aria-hidden
      data-testid="agent-status-dot"
      data-dot={!installed ? 'muted' : selected ? 'lit' : 'hollow'}
    >
      <span
        className={cn(
          'absolute inset-0 rounded-full',
          !installed && 'bg-gray-500',
          installed && selected && 'bg-green-700',
          installed && !selected && 'border-[1.5px] border-gray-500 bg-transparent',
        )}
      />
    </span>
  );
}
```

Placement: last child inside `AgentCardIdentity`, which sits in a `flex … justify-between` row inside the select `<button>` — **top-right of the identity block**, vertically nudged with `mt-0.5`.

---

## Target VI — dot spec (Task 2+ restore)

### Terminology

Compass AC-8 names “selection status dot” and “top-right status dot” separately; git history shows **one** `StatusDot` component fulfilling both roles (selection semantics + fixed top-right position). There is no second dot.

### `StatusDot` per card (when `AgentPickerStatus === 'ready'`)

| State | `data-dot` | Visual | Tokens (DESIGN.md equivalent) |
|-------|------------|--------|-------------------------------|
| Not installed | `muted` | Solid gray circle | `gray-500` fill |
| Installed, unselected | `hollow` | Hollow circle | `border-gray-500` 1.5px, transparent fill |
| Installed, selected | `lit` | Solid green circle | `green-700` fill |

**Geometry:** `h-2.5 w-2.5` (10px), `rounded-full`, `shrink-0`, `relative` wrapper + `absolute inset-0` inner disc.

**a11y:** `aria-hidden` on wrapper; `title` from i18n keys `agentPicker.status.{selected,installed,notInstalled}`.

**Test hooks:** `data-testid="agent-status-dot"`, `data-dot` attribute as above. Pre-V1.132 tests in `agent-picker.test.tsx` asserted `hollow` / `lit` / `muted` and FB-UI-006 gray-vs-green border classes.

### Card selection chrome (coexists with dot)

| Shell | Target | Rationale |
|-------|--------|-----------|
| Light | `border-2 border-blue-700` when selected; **no** `bg-blue-700/8` wash | V1.132 ring is correct cyan discipline; wash was pre-V1.132 regression in Light |
| Light | `border-2 border-gray-alpha-400` + `hover:bg-gray-alpha-100` when unselected | Neutral idle chrome |
| Dark | `border-2 border-blue-700` when selected | Cyan signal stroke — liberal per DESIGN.md |
| Both | Restore `StatusDot` alongside ring | Author requires visible dot; ring alone (V1.132 VI-001) is insufficient |

### `AgentPickerStatus` scope

| Status | Dots |
|--------|------|
| `loading` | None (spinner only) |
| `empty` | None |
| `error` | None |
| `ready` | Per-card `StatusDot` on every agent card in default + “more” grids |

---

## Cyan discipline rule (DESIGN.md-backed)

Source: `DESIGN.md` §Brand Colors — cyan usage rule; compass AC-9.

| Shell | Rule for AgentPicker |
|-------|----------------------|
| **Light** | Cyan (`blue-700` / `brand-cyan`) = **accent only**: selection **stroke** (`border-blue-700`), focus ring, loading spinner icon. **Not** card fill washes (`bg-blue-700/8` retired). Text links / retry / “more” → `text-brand-deep-blue`. Status dots use **green/gray semantic** tokens — not cyan. |
| **Dark** | Cyan **liberal**: selection stroke, focus ring, outbound links (`dark:text-blue-700`), spinner. Dots still green/gray (status semantics unchanged). |

**Token resolution note:** `blue-700` aliases `brand-cyan` in both themes after Chronos dual-role lock. Light body/link text must not use `text-blue-700` on white (AA fail ~1.9:1); `green-700` / `gray-500` on dots are valid status markers per DESIGN.md v0.4 light semantic rule (“`*-700` accents … status dots”).

---

## VI reference crosswalk

| Source | What it locks |
|--------|---------------|
| V1.107 FB-V1107-010 | Dot position top-right; lit green / hollow (later corrected to gray hollow in V1.108) |
| V1.108 FB-UI-006 | Unselected installed = hollow **gray**; selected = filled **green** — supersedes V1.107 hollow-green |
| V1.108 `ui-shell-ssot.md` §FB-UI-006 | Acceptance checklist + Studio-first invariant |
| V1.132 VI-001 fixture (`vi-aesthetic-retune-fixtures.tsx`) | **Regression artifact** — documents ring-only target; **superseded** by V1.134 P2 author requirement to restore dots |
| Author notes (compass) | “圆点都没了”; “右上角的状态圆点别删”; Light cyan accent-only |

**Mismatch resolved:** V1.132 P2 intentionally removed the dot for VI-001; V1.134 P2 reverses that decision. Restore **V1.108 `StatusDot` semantics** + **V1.132 border-2 ring** (without cyan wash).

---

## Test expectations (pre-removal baseline)

From `agent-picker.test.tsx` at `d9d7a41c^`:

- Installed unselected card exposes `[data-dot="hollow"]` with inner `border-gray-500` (not `border-green-700`)
- Selected card exposes `[data-dot="lit"]` with inner `bg-green-700`
- Not-installed card in “more” grid exposes `[data-dot="muted"]`
- V1.132 replaced these with assertions that `agent-status-dot` is **null** — Task 3 reverts to positive assertions

---

## Out of scope (T1)

- Component restore (Task 3)
- Studio fixture update (Task 2)
- Changing `@42ch/nexus-ui` Badge or Button primitives
