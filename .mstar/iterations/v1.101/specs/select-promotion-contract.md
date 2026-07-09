# Select Component Promotion — V1.101 Stretch Contract

**Status:** Locked (Execute Task 1 — 2026-07-09)  
**Document class:** Iteration-scoped contract  
**Tier:** **Stretch (P2)** — **not** Must; whole plan may defer to V1.102 without leaving Must incomplete  
**Plan:** `2026-07-09-v1.101-select-component-promotion`  
**Priority:** Stretch — starts only after P0+P1 automated paths are Done (unless PM documents capacity exception)  
**Wire:** `wire_contracts_changed: false`  
**Pattern reference:** V1.100 [form-field-contract.md](../../v1.100/specs/form-field-contract.md) + [ui-component-promotion-workflow](../../../knowledge/architecture-patterns/ui-component-promotion-workflow.md) (knowledge; compound SSOT)  
**Coordinates with:** `@42ch/nexus-ui` (`packages/nexus-ui`), `apps/web`, `apps/design-studio`, V1.100 `ui-guardrails-cn-ssot.md` P2 Wrapper/Direct-Import Rule

## 1. Goal

Promote presentational `Select` into `@42ch/nexus-ui`, following the V1.100 form-field pattern: **semantics-first**, Studio-direct imports, Web thin wrappers.

Success means contributors can tell which semantics the package owns, which the app composes, and how an accessible select is demonstrated in Design Studio — without lifting app form state into the package.

## 2. Non-Goals

- Field groups / FormField framework / React Hook Form / Zod.
- App form state managers, validation libraries, product copy helpers.
- Dialogs, combobox-as-search, multi-select product patterns, searchable lists.
- **Radix / compound Select** (`SelectTrigger` / `SelectValue` / `SelectItem` / `SelectContent` / portal popover) — deferred; see §4.1.
- Schema changes (`wire_contracts_changed: false`).
- Must-tier setup work (P0 AgentPicker, P1 wizard chrome) — those are separate plans and must not be blocked by this Stretch.
- Settings shell (**DF-70**).
- Promoting `AgentPicker` (stays app-shared in P0).

## 3. Studio-first

1. **This file** locks Select a11y/composition contract (Execute Task 1 — done when Status = Locked).
2. Studio fixtures for closed/open (native list), disabled, invalid, keyboard focus (§9 acceptance).
3. Visual acceptance → package implementation → Web wrapper / Studio consumer updates.

## 4. Implementation shape — LOCKED: native `<select>`

### 4.1 Decision

**Promote the existing native styled `<select>`** currently at `apps/web/src/components/ui/select.tsx`.

| Option | Verdict | Rationale |
|--------|---------|-----------|
| Native `<select>` + `<option>` children | **LOCKED** | Matches all current Web call sites (`create-work-dialog`, findings, canvas inspectors, annotation inspector) and Studio gallery; DESIGN.md §Input/Select/Textarea tokens; zero new package deps; preserves `value`/`onChange`/`id` HTML semantics identical to V1.100 Input |
| Radix `@radix-ui/react-select` compound (Trigger/Value/Item) | **Out of scope** | Would break every existing `<option>` call site; adds a new runtime dep; plan Non-goals already exclude combobox/multi-select product patterns; no cross-app demand for custom popover Select proven |

A future Radix compound Select requires a **separate plan/spec**. Do not smuggle Trigger/Value/Item components into this Stretch.

### 4.2 Semantic role map (trigger / value / item → native)

Plan/clarify language used “trigger / value / item.” For this locked native Select, those roles map as follows — **not** as separate package exports:

| Semantic role (clarify vocabulary) | Native ownership | Package export? |
|------------------------------------|------------------|-----------------|
| **Trigger** | The `<select>` element itself (focus target; opens the UA listbox) | Yes — single `Select` component |
| **Value** | Selected option via standard `value` / `defaultValue` props; displayed text is the selected `<option>`’s label (UA-owned) | No separate `SelectValue` — use HTML `value` |
| **Item** | Child `<option>` (or `<optgroup>` + `<option>`) elements supplied by the **app / fixture** | No `SelectItem` export — apps render `<option>` |

Package API remains a single presentational control:

```tsx
export interface SelectProps extends SelectHTMLAttributes<HTMLSelectElement> {
  invalid?: boolean;
}

export const Select = forwardRef<HTMLSelectElement, SelectProps>(...);
```

## 5. Accessibility — Locked Decisions (Task 1)

These decisions are **locked**. Tasks 2–3 implement against them — not against undocumented Radix assumptions.

### 5.1 Label / control association (`labelledby` / `htmlFor`)

Same ownership model as V1.100 form-field contract §1:

- Pair with package `Label` via **app-owned** stable `id` on `Select` and `htmlFor` on `Label`.
- Package does **not** generate, cache, or own IDs.
- Package does **not** set `aria-labelledby` internally. Consumers may pass `aria-labelledby` / `aria-label` via standard attribute passthrough when `htmlFor`+`id` is insufficient (e.g. visually hidden label patterns).
- Default Studio + Web composition: explicit `htmlFor` + `id` (not nesting).

```tsx
<Label htmlFor="work-profile">Work profile</Label>
<Select id="work-profile" value={profile} onChange={...}>
  <option value="novel">Novel</option>
</Select>
```

### 5.2 Keyboard

- Keyboard behavior is **native UA**: Tab focuses the control; Space/Enter/Alt+↓ (platform-dependent) opens the list; Arrow keys move selection; Escape closes (UA).
- Package must **not** attach custom key handlers that override native listbox behavior.
- Focus-visible styling: `focus-visible:border-blue-700` (same token path as Input/Textarea); global two-layer focus ring remains in consumer CSS / design-tokens.

### 5.3 Expanded / open state

- Open/expanded state is **UA-owned** for native `<select>`. The package does **not** export `open` / `defaultOpen` / `onOpenChange`, does not set `aria-expanded`, and does not render a custom listbox portal.
- Studio fixtures demonstrate “closed” (default) and “open” via **manual keyboard/pointer interaction** in visual acceptance — not via a package-controlled `open` prop. Automated package tests assert DOM attributes and classes on the closed control; they do not drive OS-level listbox chrome.

### 5.4 `aria-invalid` ownership

Identical to V1.100 Input/Textarea:

- Explicit `invalid?: boolean` prop:
  - `invalid={true}` → visual error border (`border-red-700`) **and** `aria-invalid="true"`.
  - `invalid={false}` or omitted → **no** `aria-invalid` attribute (`invalid || undefined` coercion).
- Native `aria-invalid` in `{...props}` may override (spread after the mapped attribute), matching Input.

### 5.5 `aria-describedby` ownership

- **App-owned wiring** — same as form-field contract §4. App generates helper/error IDs, concatenates, passes `aria-describedby`.
- Package does not generate description IDs or auto-wire helper/error children.

### 5.6 Disabled

- Pass through native `disabled` attribute.
- Presentational classes: `disabled:bg-gray-100 disabled:text-gray-700 disabled:cursor-not-allowed` (DESIGN.md `input-select-textarea.disabled`).

### 5.7 Required / optional copy

- Required/optional **indicators** are **app-owned copy** (same as form-field §5).
- Package passes through native `required` / `aria-required` when provided; does not render asterisks or “(required)” ornaments.

## 6. Package boundary (LOCKED — presentational only)

| Layer | Owns |
|-------|------|
| `@42ch/nexus-ui` `Select` | Presentational native `<select>`: DESIGN.md token styling, `invalid`/`disabled` visuals, `aria-invalid` mapping, ref forwarding, `className` merge via package `cn`, standard `SelectHTMLAttributes` passthrough |
| `apps/web` thin wrapper | `export { Select, type SelectProps } from '@42ch/nexus-ui'` under `components/ui/select.tsx`; **no** validation, copy, or daemon data |
| Apps / Studio fixtures | Options (`<option>` / `<optgroup>`), labels, form state, `value`/`onChange`, product copy, helper/error text, `aria-describedby` wiring |

**Package MUST NOT:**

- Import app routing, daemon clients, `NexusClient`, Tauri IPC, localStorage, or validation libraries.
- Export field groups, `FormField`, form context, or schema bindings.
- Export Radix Trigger/Value/Item/Content compound parts in this plan.
- Own option lists, empty-state copy, or “please select…” placeholder product strings (apps may use a disabled first `<option>` if needed).

If the contract grows beyond presentational native Select → defer or split; **do not smuggle field groups into P2**.

### 6.1 Wrapper / direct-import strategy (consume V1.100 — do not reopen)

| Surface | Strategy |
|---------|----------|
| **Web** | Thin re-export wrapper; screens keep `@/components/ui/select` imports |
| **Design Studio** | Direct `@42ch/nexus-ui` import; remove `@web-ui/select` transitional alias |
| **Guardrail** | After promotion, `select.tsx` enters `WRAPPER_CANDIDATES` / promoted set in `tooling/check-ui-guardrails.sh` |

## 7. Field groups — confirmed out of scope

Reaffirming V1.100 form-field contract §8 and this plan’s Clarify table:

- No package `FormField`, `SelectField`, or label+control+helper composite.
- No form context provider or `useFormField`.
- Composition (Label + Select + helper/error) stays **app-owned**.

## 8. Deferral rule (Must integrity)

If P0+P1 automated paths are not Done, or iteration capacity is consumed by Must work, PM may mark this plan **Deferred** and retarget to V1.102. **Deferring P2 does not make Must incomplete** and must not be reported as a Must residual.

## 9. Acceptance (only if plan runs)

- `Select` exported from `@42ch/nexus-ui` with Studio-direct + Web thin wrapper strategy matching V1.100 form-field pattern.
- Package remains presentational (no app routing/daemon/validation; no field groups).
- Implementation is native `<select>` per §4 — not Radix compound.
- Studio fixtures + package tests cover: default (closed), disabled, invalid (`aria-invalid` + border), focus-visible class path; keyboard path documented as native UA (manual visual acceptance for open list).
- Package tests cover class merging, `invalid` → `aria-invalid`, ref forwarding, and disabled attribute passthrough.
- No app code imported into `@42ch/nexus-ui`.

## 10. Human smoke

Not applicable as a Must gate for this Stretch. No interactive desktop smoke requirement for P2 automated Done.

## 11. Summary for implementers (Tasks 2–3)

| Concern | Owned by | Mechanism |
|---------|----------|-----------|
| Control rendering + visual styling | Package | Native `<select>` + DESIGN.md `input-select-textarea` tokens + `cn` |
| Options / items | App | Child `<option>` / `<optgroup>` |
| `id` / label association | App | `id` on Select + `htmlFor` on Label |
| `aria-invalid` | Package (from `invalid`) + App override | `invalid \|\| undefined` |
| `aria-describedby` | App | Concatenated helper/error IDs |
| Open/expanded | UA | No package `open` API |
| Keyboard | UA | No package key overrides |
| Validation / copy / daemon | App | Never in package |
| Field groups | Out of scope | Separate future plan if ever needed |
