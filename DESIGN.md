---
version: 0.4.0
name: "Nexus Design System"
description: "Unified design contract for all Nexus product surfaces — light/default theme. Canonical token names and values for shared visual identity; consumed by apps/web, apps/design-studio, and @42ch/nexus-ui. Dark theme uses the same token names with dark values in DESIGN.dark.md."

colors:
  # ── Brand core (VI palette — do not rename) ──
  brand-deep-blue: "#1E3A5F"
  brand-cyan: "#25D1E0"
  brand-white: "#FFFFFF"

  # ── Brand extended (hover/active/surface tints derived from VI) ──
  brand-deep-blue-800: "#182F4D"
  brand-deep-blue-900: "#12243B"
  brand-deep-blue-1000: "#0C1A2B"
  brand-cyan-800: "#1FB8C6"
  brand-cyan-900: "#1896A2"
  brand-cyan-1000: "#117480"
  brand-deep-blue-alpha-100: "rgba(30,58,95,0.08)"
  brand-deep-blue-alpha-200: "rgba(30,58,95,0.14)"
  brand-cyan-alpha-100: "rgba(37,209,224,0.12)"
  brand-cyan-alpha-200: "rgba(37,209,224,0.20)"

  # ── Neutral surfaces (apps/web parity — shipped values) ──
  # V1.121 (v0.4): background-200/300 carry a whisper of warm-paper cast
  # (The Literary Engine — atmosphere from tint, not decoration).
  background-100: "#ffffff"
  background-200: "#FAF8F4"
  background-300: "#F5F2EC"
  gray-100: "#f5f5f5"
  gray-200: "#eeeeee"
  gray-300: "#e0e0e0"
  gray-400: "#c7c7c7"
  gray-500: "#a3a3a3"
  gray-600: "#8a8a8a"
  gray-700: "#666666"
  gray-800: "#4a4a4a"
  gray-900: "#333333"
  gray-1000: "#111111"
  gray-alpha-100: "rgba(0,0,0,0.04)"
  gray-alpha-200: "rgba(0,0,0,0.06)"
  gray-alpha-300: "rgba(0,0,0,0.08)"
  gray-alpha-400: "rgba(0,0,0,0.12)"
  gray-alpha-500: "rgba(0,0,0,0.18)"
  gray-alpha-600: "rgba(0,0,0,0.24)"

  # ── Overlay scrim (elevation fill for modal/dialog/command-palette backdrops) ──
  # V1.111: dedicated scrim token — bg-gray-1000/N was theme-broken (gray-1000
  # flips to near-white in dark → light wash instead of a dimming scrim).
  scrim: "rgba(0,0,0,0.40)"

  # ── Primary interactive scale (light; maps to brand-deep-blue steps) ──
  # blue-* keys preserved as web aliases for backward compatibility.
  blue-700: "#1E3A5F"
  blue-800: "#182F4D"
  blue-900: "#12243B"
  blue-1000: "#0C1A2B"

  # ── Semantic accent scales (apps/web parity — four-step, shipped) ──
  red-700: "#e5484d"
  red-800: "#d11f2a"
  red-900: "#a91520"
  red-1000: "#7f1018"
  amber-700: "#b76e00"
  amber-800: "#935800"
  amber-900: "#704300"
  amber-1000: "#4d2d00"
  green-700: "#1f8f4d"
  green-800: "#18753e"
  green-900: "#125a30"
  green-1000: "#0d4023"
  teal-700: "#008577"
  teal-800: "#006b60"
  teal-900: "#00524a"
  teal-1000: "#003b35"
  purple-700: "#7c3aed"
  purple-800: "#6d28d9"
  purple-900: "#581cbd"
  purple-1000: "#3b1686"
  pink-700: "#db2777"
  pink-800: "#be185d"
  pink-900: "#9d174d"
  pink-1000: "#831843"

typography:
  # ── Display tier (V1.121 v0.4 — content voice) ──
  # Editorial serif for creative-entity titles, reading surfaces, and brand
  # moments. font-display = self-hosted OFL serif (Source Serif 4; subset
  # 400/600) + system-serif fallback. Serif discipline: content voice only —
  # never nav, buttons, tables, badges, or labels (§Design Concept).
  font-display: "\"Source Serif 4\", Georgia, \"Times New Roman\", ui-serif, serif"
  display-32: { fontFamily: "{typography.font-display}", fontSize: "32px", fontWeight: 600, lineHeight: 1.25, letterSpacing: "-0.01em" }
  display-24: { fontFamily: "{typography.font-display}", fontSize: "24px", fontWeight: 600, lineHeight: 1.3, letterSpacing: "-0.01em" }
  display-20: { fontFamily: "{typography.font-display}", fontSize: "20px", fontWeight: 600, lineHeight: 1.3, letterSpacing: "0" }
  heading-32: { fontFamily: "Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, \"Segoe UI\", sans-serif", fontSize: "32px", fontWeight: 650, lineHeight: 1.18, letterSpacing: "-0.025em" }
  heading-24: { fontFamily: "Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, \"Segoe UI\", sans-serif", fontSize: "24px", fontWeight: 650, lineHeight: 1.25, letterSpacing: "-0.02em" }
  heading-20: { fontFamily: "Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, \"Segoe UI\", sans-serif", fontSize: "20px", fontWeight: 600, lineHeight: 1.3, letterSpacing: "-0.015em" }
  heading-16: { fontFamily: "Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, \"Segoe UI\", sans-serif", fontSize: "16px", fontWeight: 600, lineHeight: 1.4, letterSpacing: "-0.01em" }
  label-14: { fontFamily: "Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, \"Segoe UI\", sans-serif", fontSize: "14px", fontWeight: 500, lineHeight: 1.35, letterSpacing: "0" }
  label-12: { fontFamily: "Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, \"Segoe UI\", sans-serif", fontSize: "12px", fontWeight: 600, lineHeight: 1.35, letterSpacing: "0.02em" }
  copy-16: { fontFamily: "Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, \"Segoe UI\", sans-serif", fontSize: "16px", fontWeight: 400, lineHeight: 1.6, letterSpacing: "0" }
  copy-14: { fontFamily: "Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, \"Segoe UI\", sans-serif", fontSize: "14px", fontWeight: 400, lineHeight: 1.55, letterSpacing: "0" }
  copy-13: { fontFamily: "Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, \"Segoe UI\", sans-serif", fontSize: "13px", fontWeight: 400, lineHeight: 1.5, letterSpacing: "0" }
  button-14: { fontFamily: "Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, \"Segoe UI\", sans-serif", fontSize: "14px", fontWeight: 550, lineHeight: 1, letterSpacing: "0" }
  button-12: { fontFamily: "Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, \"Segoe UI\", sans-serif", fontSize: "12px", fontWeight: 600, lineHeight: 1, letterSpacing: "0.01em" }
  label-12-mono: { fontFamily: "\"SFMono-Regular\", \"Cascadia Code\", \"Roboto Mono\", Consolas, monospace", fontSize: "12px", fontWeight: 500, lineHeight: 1.4, letterSpacing: "0" }
  copy-13-mono: { fontFamily: "\"SFMono-Regular\", \"Cascadia Code\", \"Roboto Mono\", Consolas, monospace", fontSize: "13px", fontWeight: 400, lineHeight: 1.5, letterSpacing: "0" }
  # Author Reflection — reading-surface typography.
  # Theme-independent metrics (a reading measure is a line-length target, not a
  # color); values are identical in DESIGN.dark.md so the prose column shape does
  # not shift between themes.
  reading-prose-measure: "68ch"
  reading-prose-line-height: "1.75"
  reading-prose-paragraph-spacing: "1.25em"

spacing:
  base: "4px"
  space-1: "4px"
  space-2: "8px"
  space-3: "12px"
  space-4: "16px"
  space-6: "24px"
  space-8: "32px"
  space-10: "40px"
  space-16: "64px"
  space-24: "96px"

rounded:
  control: "6px"
  card: "8px"
  popover: "12px"
  fullscreen: "16px"
  pill: "9999px"

# ── Elevation (V1.121 v0.4) ──
# Two-part shadows (ambient tight + key soft). Light theme tints shadow color
# toward ink blue (matches the T2 cast); dark theme uses pure black with
# stronger alphas. Legacy shadow-* names are preserved as aliases onto the
# scale — no consumer breakage (§Elevation).
elevation:
  elevation-0: "none"
  elevation-1: "0 1px 2px rgba(15, 23, 42, 0.04), 0 1px 3px rgba(15, 23, 42, 0.03)"
  elevation-2: "0 2px 4px rgba(15, 23, 42, 0.06), 0 4px 12px -2px rgba(15, 23, 42, 0.05)"
  elevation-3: "0 1px 1px rgba(15, 23, 42, 0.03), 0 8px 24px -12px rgba(15, 23, 42, 0.18)"
  elevation-4: "0 1px 1px rgba(15, 23, 42, 0.04), 0 24px 48px -24px rgba(15, 23, 42, 0.30)"
  shadow-card: "{elevation.elevation-1}"
  shadow-popover: "{elevation.elevation-3}"
  shadow-modal: "{elevation.elevation-4}"

# ── Motion (V1.121 v0.4) ──
# Short, standard-eased (120–220ms); reduced-motion honored per recipe (§Motion).
# duration-enter/exit are the directional pair for surfaces that appear/dismiss.
motion:
  duration-instant: "0ms"
  duration-state: "120ms"
  duration-popover: "160ms"
  duration-modal: "220ms"
  duration-enter: "200ms"
  duration-exit: "140ms"
  ease-standard: "cubic-bezier(0.16, 1, 0.3, 1)"
  ease-emphasized: "cubic-bezier(0.2, 0.8, 0.2, 1)"

components:
  # ── button: apps/web superset (tertiary + destructive + sizes + disabled) wins ──
  button:
    primary: { backgroundColor: "{colors.blue-700}", textColor: "#ffffff", borderColor: "none", rounded: "{rounded.control}", height: "40px", typography: "{typography.button-14}", hoverBackgroundColor: "{colors.blue-800}", activeBackgroundColor: "{colors.blue-900}" }
    secondary: { backgroundColor: "{colors.background-100}", textColor: "{colors.gray-1000}", borderColor: "{colors.gray-alpha-400}", rounded: "{rounded.control}", height: "40px", typography: "{typography.button-14}", hoverBackgroundColor: "{colors.background-200}", hoverBorderColor: "{colors.gray-alpha-500}" }
    tertiary: { backgroundColor: "transparent", textColor: "{colors.gray-1000}", borderColor: "none", rounded: "{rounded.control}", height: "40px", typography: "{typography.button-14}", hoverBackgroundColor: "{colors.gray-alpha-100}" }
    destructive: { backgroundColor: "{colors.red-800}", textColor: "#ffffff", borderColor: "none", rounded: "{rounded.control}", height: "40px", typography: "{typography.button-14}", hoverBackgroundColor: "{colors.red-700}", activeBackgroundColor: "{colors.red-900}" }
    sizes:
      small: { height: "32px", typography: "{typography.button-12}" }
      default: { height: "40px", typography: "{typography.button-14}" }
      large: { height: "48px", typography: "{typography.button-14}" }
    disabled: { backgroundColor: "{colors.gray-100}", textColor: "{colors.gray-700}", cursor: "not-allowed" }

  # ── focus-ring: root two-layer ring spec ──
  focus-ring:
    outer: "{colors.blue-700}"
    inner: "{colors.background-100}"
    width-outer: "2px"
    width-inner: "4px"

  # ── input-select-textarea: apps/web ──
  input-select-textarea:
    default: { backgroundColor: "{colors.background-100}", textColor: "{colors.gray-1000}", borderColor: "{colors.gray-alpha-400}", rounded: "{rounded.control}", height: "40px" }
    error: { backgroundColor: "{colors.background-100}", textColor: "{colors.gray-1000}", borderColor: "{colors.red-700}", rounded: "{rounded.control}", height: "40px" }
    disabled: { backgroundColor: "{colors.gray-100}", textColor: "{colors.gray-700}", borderColor: "{colors.gray-alpha-300}", rounded: "{rounded.control}", height: "40px" }
    textarea: { minHeight: "96px" }
    placeholder: { textColor: "{colors.gray-700}" }
    helperText: { typography: "{typography.copy-13}" }
    errorHelperText: { textColor: "{colors.red-700}", typography: "{typography.copy-13}" }

  # ── select: native `<select>` chevron inset ──
  select:
    default: { paddingInlineStart: "{spacing.space-3}", paddingInlineEnd: "{spacing.space-8}", chevronInset: "{spacing.space-3}" }

  # ── card: apps/web ──
  card:
    default: { backgroundColor: "{colors.background-100}", borderColor: "{colors.gray-alpha-400}", rounded: "{rounded.card}", padding: "{spacing.space-6}", shadow: "shadow-card" }
    compact: { padding: "{spacing.space-4}" }
    hero: { padding: "{spacing.space-8}" }
    # CardTitle voice prop (V1.121 v0.4; additive opt-in — recipe in body §Card).
    # interface = sans heading (default, unchanged); content = serif display tier.
    title:
      voice:
        interface: { typography: "{typography.heading-16}" }
        content: { typography: "{typography.display-20}" }

  # ── table: apps/web ──
  table:
    header: { backgroundColor: "{colors.background-200}", typography: "{typography.label-12}", textColor: "{colors.gray-900}", borderBottomColor: "{colors.gray-alpha-400}" }
    row: { typography: "{typography.copy-14}", textColor: "{colors.gray-1000}", secondaryTextColor: "{colors.gray-900}", hoverBackgroundColor: "{colors.background-200}", selectedBackgroundColor: "{colors.background-300}" }
    idText: { typography: "{typography.label-12-mono}" }

  # ── badge-status-pill: apps/web (tone=soft|solid; default soft) ──
  badge-status-pill:
    base: { height: "24px", paddingInline: "8px", rounded: "{rounded.pill}", typography: "{typography.label-12}", fontWeight: 600 }
    # soft (default): tinted fill + semantic text; strengthened borders (neutral gray-alpha-400; semantic ~50% alpha)
    soft:
      neutral: { backgroundColor: "{colors.gray-alpha-100}", textColor: "{colors.gray-900}", borderColor: "{colors.gray-alpha-400}" }
      running: { backgroundColor: "rgba(31,143,77,0.16)", textColor: "{colors.green-1000}", borderColor: "rgba(31,143,77,0.50)" }
      queued: { backgroundColor: "rgba(0,133,119,0.16)", textColor: "{colors.teal-1000}", borderColor: "rgba(0,133,119,0.50)" }
      warning: { backgroundColor: "rgba(183,110,0,0.16)", textColor: "{colors.amber-1000}", borderColor: "rgba(183,110,0,0.50)" }
      error: { backgroundColor: "rgba(229,72,77,0.16)", textColor: "{colors.red-1000}", borderColor: "rgba(229,72,77,0.50)" }
      preset: { backgroundColor: "rgba(124,58,237,0.16)", textColor: "{colors.purple-1000}", borderColor: "rgba(124,58,237,0.50)" }
    # solid (opt-in): semantic fill + high-contrast white text; no visible border
    solid:
      neutral: { backgroundColor: "{colors.gray-1000}", textColor: "#ffffff", borderColor: "transparent" }
      running: { backgroundColor: "{colors.green-700}", textColor: "#ffffff", borderColor: "transparent" }
      queued: { backgroundColor: "{colors.teal-700}", textColor: "#ffffff", borderColor: "transparent" }
      warning: { backgroundColor: "{colors.amber-700}", textColor: "#ffffff", borderColor: "transparent" }
      error: { backgroundColor: "{colors.red-800}", textColor: "#ffffff", borderColor: "transparent" }
      preset: { backgroundColor: "{colors.purple-700}", textColor: "#ffffff", borderColor: "transparent" }

  # ── toast: apps/web ──
  toast: { backgroundColor: "{colors.background-100}", borderColor: "{colors.gray-alpha-400}", shadow: "shadow-popover", rounded: "{rounded.popover}", maxWidth: "360px", titleTypography: "{typography.label-14}", bodyTypography: "{typography.copy-13}" }

  # ── sidebar-nav: apps/web ──
  sidebar-nav: { width: "248px", backgroundColor: "{colors.background-100}", dividerColor: "{colors.gray-alpha-400}", itemHeight: "36px", itemRounded: "{rounded.control}", itemTypography: "{typography.label-14}", activeBackgroundColor: "{colors.gray-alpha-100}", activeTextColor: "{colors.gray-1000}", activeBarColor: "{colors.blue-700}" }

  # ── listbox: apps/web ──
  listbox:
    maxHeight: "320px"

  # ── dialog / popover / sheet: apps/web ──
  dialog: { backgroundColor: "{colors.background-100}", rounded: "{rounded.popover}", shadow: "shadow-modal", maxWidth: "560px", width: "calc(100% - 2rem)", maxHeight: "85vh", padding: "{spacing.space-6}" }
  popover: { backgroundColor: "{colors.background-100}", borderColor: "{colors.gray-alpha-400}", shadow: "shadow-popover", rounded: "{rounded.popover}", itemHeight: "36px" }
  # Sheet — end-aligned drawer (work-shell right rail). Overlay uses colors.scrim (V1.121 scrim convergence).
  sheet: { backgroundColor: "{colors.background-100}", borderColor: "{colors.gray-alpha-400}", shadow: "shadow-modal", width: "min(100vw, 280px)" }

  # ── tabs: apps/web keep-web (V1.106) ──
  tabs:
    list: { backgroundColor: "{colors.background-200}", borderColor: "{colors.gray-alpha-400}", rounded: "{rounded.card}", padding: "4px", gap: "4px" }
    trigger:
      default: { typography: "{typography.button-12}", textColor: "{colors.gray-800}", height: "32px", paddingInline: "12px", rounded: "{rounded.control}" }
      hover: { backgroundColor: "{colors.gray-alpha-100}", textColor: "{colors.gray-1000}" }
      active: { backgroundColor: "{colors.background-100}", textColor: "{colors.gray-1000}", shadow: "shadow-card" }
      disabled: { textColor: "{colors.gray-700}", cursor: "not-allowed" }
      focusVisible: "{components.focus-ring}"

  # ── states: apps/web keep-web (V1.106) ──
  states:
    spinner: { size: "16px", color: "{colors.blue-700}" }
    loading: { typography: "{typography.copy-14}", textColor: "{colors.gray-700}", gap: "{spacing.space-2}", paddingBlock: "{spacing.space-6}" }
    empty:
      # V1.121 v0.4: EmptyState headline is content voice (serif display tier —
      # §Design Concept "empty-state headlines on authoring surfaces").
      titleTypography: "{typography.display-24}"
      titleColor: "{colors.gray-1000}"
      descriptionTypography: "{typography.copy-14}"
      descriptionColor: "{colors.gray-900}"
      paddingBlock: "{spacing.space-16}"
      gap: "{spacing.space-2}"
    error:
      titleTypography: "{typography.heading-16}"
      titleColor: "{colors.red-1000}"
      descriptionTypography: "{typography.copy-14}"
      descriptionColor: "{colors.red-900}"
      # backgroundColor/borderColor project to the --color-error-surface /
      # --color-error-surface-border CSS vars (bg-error-surface /
      # border-error-surface-border utilities) — V1.121 P1 T2.
      backgroundColor: "color-mix(in srgb, {colors.red-700} 6%, transparent)"
      borderColor: "color-mix(in srgb, {colors.red-700} 30%, transparent)"
      rounded: "{rounded.card}"
      padding: "{spacing.space-4}"
      retryTypography: "{typography.label-14}"
      retryColor: "{colors.blue-700}"

    # Status surface family (V1.121 P2 T4). Parallel tinted fills + borders for
    # inline status cards (preset validation results, canvas live-session
    # banner, future toast surfaces). Each projects to --color-<role>-surface /
    # --color-<role>-surface-border (bg-<role>-surface / border-<role>-surface-border
    # utilities). `error` (above) is the original; the three below complete the
    # semantic quartet so non-ui components can drop raw color-mix arbitrary
    # classes (AC-P2-4).
    success:
      backgroundColor: "color-mix(in srgb, {colors.green-700} 6%, transparent)"
      borderColor: "color-mix(in srgb, {colors.green-700} 30%, transparent)"
    warning:
      backgroundColor: "color-mix(in srgb, {colors.amber-700} 6%, transparent)"
      borderColor: "color-mix(in srgb, {colors.amber-700} 30%, transparent)"
    info:
      backgroundColor: "color-mix(in srgb, {colors.blue-700} 6%, transparent)"
      borderColor: "color-mix(in srgb, {colors.blue-700} 30%, transparent)"

    disabled:
      opacity: "0.5"

  # ── launch-daemon: apps/web (V1.106) ──
  launch-daemon:
    splash:
      titleTypography: "{typography.heading-24}"
      helperTypography: "{typography.copy-14}"
      spinnerSize: "32px"
      maxWidth: "28rem"
    main-banner:
      backgroundColor: "rgba(183,110,0,0.10)"
      borderColor: "{colors.gray-alpha-400}"
      titleTypography: "{typography.copy-14}"
      titleWeight: 600
      descriptionTypography: "{typography.copy-13}"
      paddingInline: "{spacing.space-6}"
      paddingBlock: "{spacing.space-3}"
    status-indicator: "{components.daemon-status-indicator}"

  # ── editor: apps/web ──
  editor:
    surface: "{colors.background-100}"
    surface-muted: "{colors.background-200}"
    border: "{colors.gray-alpha-400}"
    border-active: "{colors.blue-700}"
    toolbar-control-bg: "transparent"
    toolbar-control-hover: "{colors.gray-alpha-100}"
    toolbar-control-active: "{colors.gray-alpha-200}"
    save-clean: "{colors.green-700}"
    save-dirty: "{colors.amber-700}"
    save-error: "{colors.red-700}"
    selection: "rgba(0,107,255,0.14)"

  # ── data-table: apps/web ──
  data-table:
    row-hover: "{colors.background-200}"
    row-selected: "{colors.background-300}"
    row-edited: "rgba(183,110,0,0.08)"
    row-protected: "rgba(124,58,237,0.06)"
    cell-edit-bg: "{colors.background-100}"
    cell-edit-border: "{colors.blue-700}"
    column-divider: "{colors.gray-alpha-200}"

  # ── context-menu: apps/web ──
  context-menu:
    bg: "{colors.background-100}"
    border: "{colors.gray-alpha-400}"
    item-hover: "{colors.gray-alpha-100}"
    item-active: "{colors.gray-alpha-200}"
    item-disabled: "{colors.gray-700}"
    shortcut: "{colors.gray-700}"
    native-action: "{colors.gray-1000}"
    native-icon: "{colors.gray-900}"
    native-disabled: "{colors.gray-700}"
    native-danger: "{colors.red-700}"

  # ── desktop-window-chrome: apps/web ──
  desktop-window-chrome:
    window-bg: "{colors.background-100}"
    window-border: "{colors.gray-alpha-400}"
    titlebar-safe-area: "28px"
    window-radius: "{rounded.card}"
    window-drag-region-height: "0px"

  # ── app-menu: apps/web ──
  app-menu:
    label: "{colors.gray-1000}"
    secondary: "{colors.gray-700}"
    disabled: "{colors.gray-700}"
    danger: "{colors.red-700}"

  # ── native-dialogs: apps/web ──
  native-dialogs:
    title: "{typography.heading-20}"
    body: "{typography.copy-14}"
    secondary: "{colors.gray-900}"
    danger: "{colors.red-700}"
    warning: "{colors.amber-700}"

  # ── daemon-status-indicator: apps/web ──
  daemon-status-indicator:
    healthy-bg: "rgba(31,143,77,0.10)"
    healthy-text: "{colors.green-1000}"
    starting-bg: "rgba(0,133,119,0.10)"
    starting-text: "{colors.teal-1000}"
    degraded-bg: "rgba(183,110,0,0.12)"
    degraded-text: "{colors.amber-1000}"
    stopped-bg: "rgba(229,72,77,0.12)"
    stopped-text: "{colors.red-1000}"

  # ── shell-nav: root (brand navigation tokens) ──
  shell-nav:
    logo-variant: "logo-primary.svg"
    logo-min-height: "24px"
    logo-clear-space-ratio: "0.25"
    active-bar-color: "{colors.blue-700}"
    brand-mark-on-light: "{colors.brand-deep-blue}"
    brand-accent-on-light: "{colors.brand-cyan}"

  # ── logo: root ──
  logo:
    min-size-px: "24"
    clear-space-ratio: "0.25"
    alt-text: "Nexus"

  # ── connection-setup: root (V1.92 P-1 T4) ──
  connection-setup:
    security-note:
      backgroundColor: "{colors.brand-deep-blue-alpha-100}"
      borderColor: "{colors.brand-deep-blue-alpha-200}"
      textColor: "{colors.gray-900}"
      borderWidth: "1px"
      rounded: "{rounded.card}"
      padding: "{spacing.space-4}"
      iconColor: "{colors.brand-deep-blue}"
    warning:
      backgroundColor: "rgba(183,110,0,0.08)"
      borderColor: "rgba(183,110,0,0.20)"
      textColor: "{colors.gray-900}"
      borderWidth: "1px"
      rounded: "{rounded.card}"
      padding: "{spacing.space-4}"
      iconColor: "{colors.amber-700}"
    info-note:
      backgroundColor: "{colors.gray-alpha-100}"
      borderColor: "{colors.gray-alpha-300}"
      textColor: "{colors.gray-800}"
      borderWidth: "1px"
      rounded: "{rounded.card}"
      padding: "{spacing.space-4}"
      iconColor: "{colors.gray-600}"
    fingerprint-block:
      backgroundColor: "{colors.background-200}"
      borderColor: "{colors.gray-alpha-400}"
      textColor: "{colors.gray-1000}"
      borderWidth: "1px"
      rounded: "{rounded.control}"
      padding: "{spacing.space-3}"
      fontFamily: "\"SFMono-Regular\", \"Cascadia Code\", \"Roboto Mono\", Consolas, monospace"
      fontSize: "13px"
      fontWeight: 400
      lineHeight: 1.5
    form-gap: "{spacing.space-6}"
    form-section-gap: "{spacing.space-8}"
    label-gap: "{spacing.space-1}"
    helper-text-size: "13px"
    helper-text-color: "{colors.gray-600}"

  # ── finding-status-pill: apps/web ──
  finding-status-pill:
    open: { backgroundColor: "rgba(183,110,0,0.12)", textColor: "{colors.amber-1000}", borderColor: "rgba(183,110,0,0.30)" }
    triaged: { backgroundColor: "rgba(0,133,119,0.10)", textColor: "{colors.teal-1000}", borderColor: "rgba(0,133,119,0.30)" }
    in_review: { backgroundColor: "rgba(0,107,255,0.10)", textColor: "{colors.blue-1000}", borderColor: "rgba(0,107,255,0.30)" }
    resolved: { backgroundColor: "rgba(31,143,77,0.10)", textColor: "{colors.green-1000}", borderColor: "rgba(31,143,77,0.30)" }
    wont_fix: { backgroundColor: "{colors.gray-alpha-100}", textColor: "{colors.gray-900}", borderColor: "{colors.gray-alpha-300}" }
    duplicate: { backgroundColor: "rgba(124,58,237,0.10)", textColor: "{colors.purple-1000}", borderColor: "rgba(124,58,237,0.30)" }
    base: { height: "24px", paddingInline: "8px", rounded: "{rounded.pill}", typography: "{typography.label-12}" }

  # ── finding-triage: apps/web ──
  finding-triage:
    panel-bg: "{colors.background-100}"
    panel-border: "{colors.gray-alpha-400}"
    row-active: "{colors.background-300}"
    action-button: "secondary"
    executor-select: "input-select-textarea.default"

  # ── memory: apps/web ──
  memory-pending-count:
    backgroundColor: "rgba(229,72,77,0.12)"
    textColor: "{colors.red-1000}"
    borderColor: "rgba(229,72,77,0.30)"
    base: { height: "20px", minInlineSize: "20px", paddingInline: "6px", rounded: "{rounded.pill}", typography: "{typography.label-12}" }
  memory-review-button:
    basis: "primary"
  memory-task-kind-brainstorm:
    backgroundColor: "rgba(183,110,0,0.12)"
    textColor: "{colors.amber-1000}"
    borderColor: "rgba(183,110,0,0.30)"
  memory-task-kind-outline:
    backgroundColor: "rgba(0,107,255,0.10)"
    textColor: "{colors.blue-1000}"
    borderColor: "rgba(0,107,255,0.30)"
  memory-task-kind-chapter:
    backgroundColor: "rgba(0,133,119,0.10)"
    textColor: "{colors.teal-1000}"
    borderColor: "rgba(0,133,119,0.30)"
  memory-task-kind-research:
    backgroundColor: "rgba(124,58,237,0.10)"
    textColor: "{colors.purple-1000}"
    borderColor: "rgba(124,58,237,0.30)"
  memory-task-kind-unknown:
    backgroundColor: "{colors.gray-alpha-100}"
    textColor: "{colors.gray-900}"
    borderColor: "{colors.gray-alpha-300}"
  memory-task-kind-base: { height: "24px", paddingInline: "8px", rounded: "{rounded.pill}", typography: "{typography.label-12}" }
  memory-fragment-summary:
    typography: "{typography.copy-14}"
  memory-fragment-id:
    typography: "{typography.copy-13-mono}"
    textColor: "{colors.gray-800}"
  memory-inspector-header:
    panel-bg: "{colors.background-100}"
    panel-border: "{colors.gray-alpha-400}"
    row-active: "{colors.background-300}"
  memory-inspector-field-label:
    typography: "{typography.label-14}"
    textColor: "{colors.gray-900}"
  memory-inspector-field-value:
    typography: "{typography.copy-13}"
    textColor: "{colors.gray-1000}"
  memory-fragment-filter-input:
    basis: "input-select-textarea.default"

  # ── reading-chapter-nav: apps/web ──
  reading-chapter-nav:
    chrome-bg: "{colors.background-200}"
    chrome-border: "{colors.gray-alpha-400}"
    control-prev: "button.secondary basis"
    control-next: "button.secondary basis"
    volume-group-bg: "{colors.background-300}"
    volume-group-border: "{colors.gray-alpha-300}"

  # ── reading-progress-indicator: apps/web ──
  reading-progress-indicator:
    track: "{colors.gray-alpha-200}"
    fill: "{colors.blue-700}"
    label: "{colors.gray-700}"

  # ── reading-maturation-badge: apps/web ──
  reading-maturation-badge:
    chapter-completion-state: "ChapterStatusBadge basis"
    world-kb-density-count: { backgroundColor: "rgba(0,133,119,0.10)", textColor: "{colors.teal-1000}", borderColor: "rgba(0,133,119,0.30)" }
    open-findings-count: { backgroundColor: "rgba(183,110,0,0.12)", textColor: "{colors.amber-1000}", borderColor: "rgba(183,110,0,0.30)" }
    base: { height: "20px", paddingInline: "6px", rounded: "{rounded.pill}", typography: "{typography.label-12}" }

  # ── SOUL visualization: apps/web ──
  soul-viz-keyword-cluster-node:
    shape: "circle"
    size: "min-max 10px-44px by frequency"
    fill: "rgba(124,58,237,0.18)"
    stroke: "{colors.purple-700}"
    label: "{colors.gray-1000}"
  soul-viz-timeline-axis:
    line: "{colors.gray-alpha-400}"
    tick: "{colors.gray-400}"
    label: "{typography.label-12} @ {colors.gray-700}"
  soul-viz-drift-band:
    fill: "rgba(0,107,255,0.16)"
    fill-2: "rgba(124,58,237,0.16)"
    fill-3: "rgba(0,133,119,0.16)"
    fill-4: "rgba(183,110,0,0.16)"
    fill-5: "rgba(190,24,93,0.16)"
    fill-6: "rgba(53,142,53,0.16)"
    step-stroke: "{colors.gray-alpha-200}"
    label: "{typography.label-12} @ {colors.gray-900}"
  soul-narrative-prose: "{typography.copy-16} @ {colors.gray-900}"
  soul-growth-curve-stroke: "{colors.purple-700}"

  # ── canvas: apps/web ──
  # V1.121 (v0.4) chromatic hygiene: every Tailwind-palette leftover hex is
  # remapped hue-preserving onto the brand semantic scales — see
  # §Appendix: Canvas Chromatic Hygiene Mapping. Ambient surfaces carry the
  # warm-paper whisper (light) per §Design Concept.
  canvas:
    canvas-surface: "#EBE9E5"
    canvas-grid: "rgba(0,0,0,0.05)"
    canvas-grid-gap: "20px"
    canvas-grid-dot-size: "1.5px"
    canvas-node-fill: "#ffffff"
    canvas-node-fill-hover: "{colors.background-300}"
    canvas-node-border: "rgba(0,0,0,0.14)"
    canvas-node-border-selected: "{colors.blue-700}"
    canvas-edge: "{colors.gray-500}"
    canvas-edge-hover: "{colors.gray-800}"
    canvas-port: "{colors.gray-700}"
    canvas-minimap: "{colors.gray-alpha-600}"
    canvas-strategy-accent: "{colors.purple-700}"
    # V1.121 P3 T2: per-surface accent spines (§Canvas Surface — strategy =
    # purple-700, outline = amber-700, World KB = teal-700). Each surface's
    # spine is its own token so retuning one does not bleed into the others.
    canvas-outline-accent: "{colors.amber-700}"
    canvas-worldkb-accent: "{colors.teal-700}"
    # V1.123 P3 T2: Timeline accent spine. Timeline is the central instrument
    # (iterations/v1.123/specs/three-layer-product-spec.md); brand-blue per the
    # Canvas/SOUL invariant. Mirror the per-surface pattern.
    canvas-timeline-accent: "{colors.blue-700}"
    # V1.123 P4 Task 2: per-layer feel accents (layer-feel-differentiation.md
    # §6.1 — three-layer feel contract for AC-V1123-20 "three feels
    # perceptibly different"). Brief=gold-bronze age tone (amber-700 alias —
    # Outline lives on a separate surface so the shared hue does not collide);
    # Narrative=brand blue (aliases the V1.123 P3 Timeline accent — Narrative
    # is the shared V1.122 baseline within the Timeline surface family);
    # Moment=ink-on-paper manuscript tone (gray-900 alias — gray-900 projects
    # to #e0e0e0 in dark theme so the ink hue stays legible on inverted dark
    # canvas). No new palette color invented; tuning later is a token-only
    # edit, not a node-component sweep.
    canvas-layer-brief-accent: "{colors.amber-700}"
    canvas-layer-narrative-accent: "{colors.blue-700}"
    canvas-layer-moment-accent: "{colors.gray-900}"
    canvas-write-dirty: "{colors.amber-700}"
    canvas-write-conflict: "{colors.red-700}"
    canvas-write-success: "{colors.green-700}"
    canvas-write-stale-bg: "color-mix(in srgb, {colors.amber-700} 8%, transparent)"
    # Canvas node width family (V1.121 v0.4 contract; registered in pipeline P0
    # as structural --canvas-node-width-* vars, applied to node components P3 — values grep-verified from source)
    node-width:
      strategy-root: "260px"
      strategy-primary: "140px"
      strategy-secondary: "150px"
      outline-scene-beat: "160px"
      default: "176px"
    # Outline/timeline
    canvas-outline-volume-fill: "#F5F5F4"
    canvas-outline-chapter-card-status-pending: "{colors.gray-500}"
    canvas-outline-chapter-card-status-drafted: "{colors.blue-700}"
    canvas-outline-chapter-card-status-completed: "{colors.green-700}"
    canvas-outline-timeline-event-pin: "{colors.amber-700}"
    canvas-outline-foreshadow-edge: "{colors.purple-700}"
    canvas-outline-timeline-marker: "{colors.teal-700}"
    canvas-outline-conflict-marker: "{colors.red-700}"
    # Outline Scene/Beat (V1.109 C2 — FB-C2-001)
    canvas-outline-scene-fill: "#FAFAF9"
    canvas-outline-scene-border: "rgba(0,0,0,0.14)"
    canvas-outline-scene-status-drafted: "{colors.blue-700}"
    canvas-outline-scene-status-completed: "{colors.green-700}"
    canvas-outline-beat-fill: "#F5F5F4"
    canvas-outline-beat-border: "rgba(0,0,0,0.14)"
    # World KB
    canvas-worldkb-entity-card-fill-default: "#FFFFFF"
    canvas-worldkb-entity-card-fill-hover: "#F5F5F5"
    canvas-worldkb-entity-card-fill-selected: "#EBF2FF"
    canvas-worldkb-entity-card-stroke-default: "rgba(0,0,0,0.14)"
    canvas-worldkb-entity-card-stroke-selected: "{colors.blue-700}"
    canvas-worldkb-promotion-pending: "{colors.amber-700}"
    canvas-worldkb-promotion-confirmed: "{colors.green-700}"
    canvas-worldkb-promotion-rejected: "{colors.red-700}"
    canvas-worldkb-promotion-merged: "{colors.purple-700}"
    canvas-worldkb-source-anchor-edge: "{colors.purple-700}"
    canvas-worldkb-source-anchor-node: "color-mix(in srgb, {colors.purple-700} 10%, transparent)"
    canvas-worldkb-computable-badge: "{colors.teal-700}"
    canvas-worldkb-conflict-marker: "{colors.red-700}"
    canvas-worldkb-conflict-marker-fill: "color-mix(in srgb, {colors.red-700} 10%, transparent)"
    canvas-worldkb-nonspatial-row-highlight: "#F5F5F4"
    canvas-worldkb-focus-ring: "{colors.blue-700}"
    canvas-worldkb-relationship-edge: "{colors.gray-500}"
    canvas-worldkb-relationship-edge-default: "{colors.gray-500}"
    canvas-worldkb-relationship-edge-symmetric: "{colors.purple-700}"
    canvas-worldkb-relationship-edge-custom: "{colors.pink-700}"
    canvas-worldkb-relationship-confidence-low: "{colors.red-700}"
    canvas-worldkb-relationship-confidence-mid: "{colors.amber-700}"
    canvas-worldkb-relationship-confidence-high: "{colors.green-700}"
    canvas-worldkb-relationship-grounded-badge: "color-mix(in srgb, {colors.blue-700} 12%, transparent)"
    canvas-worldkb-relationship-asserted-badge: "rgba(124,58,237,0.12)"
    canvas-worldkb-relationship-inspector-fill: "#FFFFFF"

  # ── reading annotations (V1.89): apps/web ──
  reading-annotation-highlight-yellow:
    backgroundColor: "color-mix(in srgb, {colors.amber-700} 18%, transparent)"
    textColor: "{colors.amber-1000}"
  reading-annotation-highlight-blue:
    backgroundColor: "color-mix(in srgb, {colors.blue-700} 12%, transparent)"
    textColor: "{colors.blue-1000}"
  reading-annotation-highlight-green:
    backgroundColor: "color-mix(in srgb, {colors.green-700} 16%, transparent)"
    textColor: "{colors.green-1000}"
  reading-annotation-highlight-pink:
    backgroundColor: "color-mix(in srgb, {colors.pink-700} 16%, transparent)"
    textColor: "{colors.pink-1000}"
  reading-annotation-inspector:
    width: "320px"
    backgroundColor: "{colors.background-100}"
    borderColor: "{colors.gray-alpha-400}"
    textColor: "{colors.gray-1000}"
    elevation: "shadow-elevation-3"
  reading-selection-toolbar:
    backgroundColor: "{colors.background-100}"
    borderColor: "{colors.gray-alpha-400}"
    textColor: "{colors.gray-1000}"
    shadow: "0px 4px 12px rgba(0,0,0,0.12)"

  # ── reading chrome (V1.91; V1.121 v0.4 — display serif absorbed into
  # typography.font-display, raw rgba tints absorbed into semantic scales) ──
  reading-chrome-novel:
    chapter-title:
      fontFamily: "{typography.font-display}"
      fontSize: "28px"
      fontWeight: 600
      lineHeight: 1.3
      letterSpacing: "-0.01em"
      color: "{colors.gray-1000}"
    scene-separator:
      color: "{colors.gray-500}"
      fontSize: "16px"
      textAlign: "center"
      paddingBlock: "12px"
    epigraph:
      fontStyle: "italic"
      textAlign: "right"
      color: "{colors.gray-700}"
      paddingLeft: "25%"
  reading-chrome-essay:
    section-heading:
      fontFamily: "Inter, ui-sans-serif, system-ui, -apple-system, sans-serif"
      fontSize: "18px"
      fontWeight: 500
      lineHeight: 1.4
      letterSpacing: "-0.01em"
      color: "{colors.gray-1000}"
      marginTop: "28px"
    blockquote:
      borderLeft: "3px solid {colors.gray-alpha-400}"
      paddingLeft: "16px"
      fontStyle: "italic"
      color: "{colors.gray-900}"
    footnote-marker:
      verticalAlign: "super"
      fontSize: "0.75em"
      color: "{colors.teal-700}"
  reading-chrome-game-bible:
    term-link:
      color: "{colors.teal-700}"
      textDecoration: "underline dotted"
      cursor: "pointer"
    definition-callout:
      backgroundColor: "color-mix(in srgb, {colors.teal-700} 6%, transparent)"
      borderLeft: "3px solid {colors.teal-700}"
      padding: "12px 16px"
      labelColor: "{colors.teal-900}"
      labelFontWeight: 600
    category-badge:
      backgroundColor: "color-mix(in srgb, {colors.amber-700} 12%, transparent)"
      textColor: "{colors.amber-1000}"
  reading-chrome-script:
    character-name:
      textAlign: "center"
      textTransform: "uppercase"
      fontWeight: 700
      fontSize: "14px"
      letterSpacing: "0.08em"
      marginTop: "20px"
      color: "{colors.gray-1000}"
    parenthetical:
      fontStyle: "italic"
      color: "{colors.gray-700}"
      paddingLeft: "32px"
    scene-heading:
      fontWeight: 700
      textTransform: "uppercase"
      fontSize: "14px"
      letterSpacing: "0.05em"
      color: "{colors.gray-900}"
      marginTop: "24px"

  # ── footer-profile (V1.94): apps/web ──
  footer-profile:
    avatar-size: "32px"
    avatar-rounded: "{rounded.pill}"
    avatar-bg: "{colors.gray-alpha-100}"
    avatar-bg-hover: "{colors.gray-alpha-200}"
    avatar-bg-active: "{colors.blue-700}"
    avatar-text: "{colors.gray-1000}"
    avatar-text-active: "#ffffff"
    avatar-fallback-bg: "{colors.gray-alpha-200}"
    avatar-fallback-text: "{colors.gray-700}"
    add-button-bg: "transparent"
    add-button-border: "{colors.gray-alpha-400}"
    add-button-text: "{colors.gray-700}"
    add-button-hover-bg: "{colors.gray-alpha-100}"
    add-button-hover-border: "{colors.gray-alpha-500}"
    add-button-hover-text: "{colors.gray-1000}"
    gap: "{spacing.space-2}"

  # ── setup-wizard-step (V1.94): apps/web ──
  setup-wizard-step:
    step-row-height: "40px"
    step-circle-size: "32px"
    step-circle-active-bg: "{colors.blue-700}"
    step-circle-active-text: "#ffffff"
    step-circle-complete-bg: "{colors.green-700}"
    step-circle-complete-text: "#ffffff"
    step-circle-pending-bg: "{colors.gray-alpha-100}"
    step-circle-pending-text: "{colors.gray-700}"
    step-connector: "{colors.gray-alpha-400}"
    step-label-typography: "{typography.label-14}"
    step-label-active-color: "{colors.gray-1000}"
    step-label-pending-color: "{colors.gray-700}"
    wizard-max-width: "480px"
    wizard-max-height: "720px"
    wizard-padding: "{spacing.space-8}"

  # ── setup-wizard-surface (V1.96): apps/web ──
  setup-wizard-surface:
    card-bg: "{colors.background-100}"
    card-border: "{colors.gray-alpha-400}"
    card-shadow: "shadow-modal"
    card-rounded: "{rounded.popover}"
    step-panel-width: "208px"
    step-panel-right-divider: "{colors.gray-alpha-200}"
    step-panel-padding-x: "{spacing.space-6}"
    step-panel-padding-y: "{spacing.space-8}"
    content-panel-padding-x: "{spacing.space-10}"
    content-panel-padding-y: "{spacing.space-8}"
    input-row-gap: "{spacing.space-3}"
    input-row-min-height: "48px"
    input-row-bg: "{colors.background-200}"
    input-row-border: "{colors.gray-alpha-400}"
    input-row-rounded: "{rounded.control}"
    input-row-padding-x: "{spacing.space-4}"
    input-row-padding-y: "{spacing.space-3}"
    input-row-label-color: "{colors.gray-700}"
    input-row-path-color: "{colors.gray-1000}"
    input-row-icon-color: "{colors.blue-700}"
    cta-primary-max-width: "400px"
    cta-container-gap: "{spacing.space-4}"
    step-transition-duration: "duration-state"
---

# Nexus Design System

<!-- COMPLETENESS_LEVEL: 3 — Production, last audited 2026-07-17 (v0.4 Literary Engine re-audit) -->

**This file is the sole normative design SSOT for all Nexus product surfaces.** It defines canonical token names, values, accessibility intent, logo rules, voice guidance, and component-level design tables for `apps/web`, `apps/design-studio`, and `@42ch/nexus-ui`. Dark theme uses the same token names with dark values in [`DESIGN.dark.md`](DESIGN.dark.md).

**Token hierarchy:**

1. Root `DESIGN.md` / `DESIGN.dark.md` — sole design SSOT (this file)
2. `packages/nexus-ui` (`@42ch/nexus-ui`) — package-consumable brand tokens, `theme.css`, logo assets
3. `tooling/design-tokens` (`@nexus/design-tokens`) — shared Tailwind preset + `tokens.css` for all consumers
4. `apps/web`, `apps/design-studio` — product surfaces that consume the CSS pipeline

**Author experience intent:** Nexus should feel like a calm, trustworthy creative workspace — recognizable from the shell, low-distraction, with clear action hierarchy. Brand visibility targets navigation identity, focus states, and base primitives — not full-page redesign.

Nexus Local Web UI is a restrained, author-focused design system for the local-first **Control Room + Setup + Authoring** SPA. It should feel calm and trustworthy: quiet surfaces, dense but readable data, explicit status language, and high-confidence controls for local creative runtime work without making writers feel like they are operating infrastructure.

Product inputs from `.mstar/specs/web-ui-design-requirements.md`:

- Primary persona: writers/authors, not engineers; calm and focused over dashboard anxiety.
- Control Room screens are data-dense; Setup screens are form-dense with first-class validation and destructive-action confirmation.
- Authoring screens include outline editing, chapter structure tables, and a body read-only context menu.
- WCAG 2.1 AA is the floor in both light and dark; focus rings, keyboard paths, status text, and reduced motion are non-negotiable.
- Brand voice: helpful, plain, local-first, and consistent with CLI terms (`Work`, `preset`, `stage`, `finding`, `capability`).

---

## Design Concept — The Literary Engine (V1.121, v0.4)

Nexus is a **writer's atelier resting on a computational engine**. The design language reconciles two registers:

- **Content voice (文学 / 创作):** editorial serif (`font-display` + the `display-*` tier) for creative-entity titles, reading surfaces, and brand moments. Serif = the author's material.
- **Interface voice (AI / 互联 / 画布 / 计算引擎):** precise system sans, ink-blue atmospheric darks, cyan signal accents, instrument-grade status language. Sans = the engine.

Rules that keep the concept premium rather than themed:

1. **Serif discipline.** Serif appears **only** in content-voice positions: work/world/chapter titles, manuscript headings, empty-state headlines on authoring surfaces, and the brand page. **Never** in nav, buttons, tables, badges, or labels. Opt-in is explicit and greppable (e.g. `components.card.title.voice` = `content`).
2. **Atmosphere from tint, not decoration.** The dark theme is an *ink chamber*: `background-100…300` and `gray-100…300` carry a deep-blue cast derived from `brand-deep-blue-1000`. Light surfaces keep near-white calm with a whisper of warm-paper cast in `background-200/300` and the canvas. No gradients-as-decoration, no noise textures, no glassmorphism.
3. **Depth is functional.** The `elevation-0…4` scale communicates interactivity (rest → hover → pressed → dragging), not ornament. See §Elevation for the per-component recipes.
4. **Motion is short and standard-eased.** 120–220ms, `duration-enter`/`duration-exit` directional pairs, `prefers-reduced-motion` honored per recipe. See §Motion.

---

## Brand Colors

VI palette (frozen names):

| Token | Hex | Role |
| --- | --- | --- |
| `brand-deep-blue` | `#1E3A5F` | Primary brand, primary actions on light surfaces, links, focus rings |
| `brand-cyan` | `#25D1E0` | Accent — icons, active indicators, dark-theme interactive emphasis |
| `brand-white` | `#FFFFFF` | Text on deep blue fills, logo on dark hero surfaces |

Extended brand steps (`brand-deep-blue-800` … `brand-cyan-1000`, `brand-deep-blue-alpha-*`, `brand-cyan-alpha-*`) support hover, active, and low-opacity washes without raw VI drift.

### Contrast review (WCAG 2.1 AA — light theme)

| Pairing | Ratio | Intended usage | Verdict |
| --- | --- | --- | --- |
| `brand-deep-blue` on `background-100` | 11.5:1 | Headings, links, primary button label context | **Pass** — body text OK |
| `brand-white` on `brand-deep-blue` | 11.5:1 | Primary button label | **Pass** |
| `brand-cyan` on `background-100` | 1.9:1 | — | **Fail** — accent/icon/active indicator only; **never body text on white** |
| `brand-cyan` on `brand-deep-blue` | 6.2:1 | Accent chips on brand panels | **Pass** |
| `gray-1000` on `background-100` | 18.9:1 | Primary UI text | **Pass** |
| `gray-700` on `background-100` | 5.7:1 | Secondary/helper text | **Pass** |

**Cyan usage rule:** `brand-cyan` is an accent token. Use it for marks, borders, icons, progress fills, and dark-theme interactive emphasis — not for paragraph text on white or light gray.

### Background-driven contrast invariant

Text color on any filled element is decided by the **perceived lightness of that element's background**, not by the active light/dark mode. Dark backgrounds (deep blue, dark red, dark gray, saturated dark colors) use light text; light/bright backgrounds (cyan, light red, light green, pastel tints) use dark text. The same component may need dark text in both themes if its background stays light/bright in both.

---

## Colors

Color values live in frontmatter `colors:`. Color tokens follow the Geist-style intent scale: `100` background/quiet, `400` border, `700` solid fill, `900` secondary text, `1000` primary text. Use color for state and hierarchy, not decoration.

- Background values: see frontmatter `colors.background-*`. Background scale encodes surface hierarchy: `100` default, `200` subtle panel/table header, `300` hover/selected.
- Gray values: see frontmatter `colors.gray-*`. Solid gray carries text, icons, disabled fills, and opaque border fallback.
- Gray-alpha values: see frontmatter `colors.gray-alpha-*`. Alpha gray carries hover wash, separators, active wash, borders, and dividers over either theme.
- Accent values: see frontmatter `colors.blue-*`, `red-*`, `amber-*`, `green-*`, `teal-*`, `purple-*`, and `pink-*`. Accent color carries semantic state and should not be decorative.

### Semantic Mapping

| Meaning | Token |
| --- | --- |
| Primary action/focus/link | `blue-700` → maps to `brand-deep-blue` in light |
| Brand accent (icons, dark nav; not body text on white) | `brand-cyan` |
| Running/healthy/completed | `green-700` |
| Warning/stale/needs review | `amber-700` |
| Failed/error/destructive | `red-700` / `red-800` |
| Informational/queued | `teal-700` |
| Preset/capability metadata | `purple-700` |

### Brand → Web alias map (light)

| Root token | Web frontmatter key | CSS variable |
| --- | --- | --- |
| `brand-deep-blue` | `brand-deep-blue`, `blue-700` | `--color-brand-deep-blue`, `--color-blue-700` |
| `brand-deep-blue-800` | `blue-800` | `--color-blue-800` |
| `brand-deep-blue-900` | `blue-900` | `--color-blue-900` |
| `brand-cyan` | `brand-cyan` | `--color-brand-cyan` |
| `brand-white` | `brand-white` | `--color-brand-white` |
| Package mirror | — | `--nexus-brand-deep-blue` via `@42ch/nexus-ui/theme.css` |

`blue-*` keys are **preserved aliases** so existing component tokens (`{colors.blue-700}`) continue to resolve without renames.

---

## Neutrals

Neutral tokens (`background-*`, `gray-*`, `gray-alpha-*`) are refined for UI contrast and are not part of the VI brief. They carry surface hierarchy, borders, and text — brand colors carry identity and primary action.

**Atmosphere (V1.121):** neutrals are not hue-less. Dark-theme `background-100…300` + `gray-100…300` take a deep-blue cast derived from `brand-deep-blue-1000` (ink chamber); light-theme `background-200/300` take a whisper of warm paper. Lightness stays matched to the pre-v0.4 neutrals so every previously-passing text pairing keeps its AA verdict — see §Contrast (AA, recomputed).

---

## Contrast (AA, recomputed — V1.121)

Full WCAG 2.1 AA recomputation for the v0.4 ink (dark) and warm-paper (light) surfaces. Every changed pairing appears below; candidates were gated cell-by-cell — **no previously-passing pairing regressed**, so the spec T2 candidate values locked unmodified (no ±1 lightness tuning was needed). Method: WCAG relative luminance over sRGB; alpha overlays resolved to effective colors.

**Verdict key:** **P** = Pass normal text (≥ 4.5:1) · **G** = graphical / large-text only (3.0–4.49:1; icons, markers, borders, ≥ 24px or ≥ 18.66px bold text) · **F** = Fail (< 3.0:1 — decorative/non-text use only).

### Dark theme (ink surfaces)

| Text token | `background-100` `#0A1320` | `background-200` `#0F1A2A` | `background-300` `#152438` | `gray-100` `#141F2E` | `gray-200` `#1E2A3D` | `gray-300` `#283749` | `canvas-surface` `#101D2E` | `scrim` (eff. `#04080D`) |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `gray-1000` `#f5f5f5` | 17.1 **P** | 16.0 **P** | 14.4 **P** | 15.2 **P** | 13.3 **P** | 11.1 **P** | 15.6 **P** | 18.4 **P** |
| `gray-900` `#e0e0e0` | 14.1 **P** | 13.2 **P** | 11.9 **P** | 12.6 **P** | 10.9 **P** | 9.2 **P** | 12.9 **P** | 15.2 **P** |
| `gray-700` `#a3a3a3` | 7.4 **P** | 6.9 **P** | 6.2 **P** | 6.6 **P** | 5.7 **P** | 4.8 **P** | 6.7 **P** | 8.0 **P** |
| `gray-500` `#737373` | 3.9 **G** | 3.7 **G** | 3.3 **G** | 3.5 **G** | 3.1 **G** | 2.6 **F** | 3.6 **G** | 4.2 **G** |
| `brand-cyan` = `blue-700` `#25D1E0` | 10.0 **P** | 9.4 **P** | 8.4 **P** | 8.9 **P** | 7.8 **P** | 6.5 **P** | 9.1 **P** | 10.8 **P** |
| `brand-deep-blue` `#1E3A5F` | 1.6 **F** | 1.5 **F** | 1.4 **F** | 1.4 **F** | 1.3 **F** | 1.1 **F** | 1.5 **F** | 1.8 **F** |
| `red-700` `#ff6b6b` | 6.7 **P** | 6.3 **P** | 5.6 **P** | 6.0 **P** | 5.2 **P** | 4.4 **G** | 6.1 **P** | 7.2 **P** |
| `amber-700` `#ffc043` | 11.4 **P** | 10.7 **P** | 9.6 **P** | 10.2 **P** | 8.9 **P** | 7.4 **P** | 10.4 **P** | 12.3 **P** |
| `green-700` `#54d58a` | 10.0 **P** | 9.4 **P** | 8.4 **P** | 8.9 **P** | 7.8 **P** | 6.5 **P** | 9.1 **P** | 10.8 **P** |
| `teal-700` `#4cd8c8` | 10.6 **P** | 10.0 **P** | 8.9 **P** | 9.5 **P** | 8.2 **P** | 6.9 **P** | 9.7 **P** | 11.4 **P** |
| `purple-700` `#b794ff` | 7.7 **P** | 7.2 **P** | 6.5 **P** | 6.9 **P** | 6.0 **P** | 5.0 **P** | 7.0 **P** | 8.3 **P** |
| `pink-700` `#ff8ac2` | 8.6 **P** | 8.1 **P** | 7.2 **P** | 7.6 **P** | 6.7 **P** | 5.6 **P** | 7.8 **P** | 9.2 **P** |
| `brand-white` `#FFFFFF` | 18.6 **P** | 17.5 **P** | 15.7 **P** | 16.6 **P** | 14.5 **P** | 12.1 **P** | 17.0 **P** | 20.1 **P** |

### Light theme (warm-paper surfaces)

| Text token | `background-100` `#ffffff` | `background-200` `#FAF8F4` | `background-300` `#F5F2EC` | `gray-100` `#f5f5f5` | `gray-200` `#eeeeee` | `gray-300` `#e0e0e0` | `canvas-surface` `#EBE9E5` | `scrim` (eff. `#999999`) |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `gray-1000` `#111111` | 18.9 **P** | 17.8 **P** | 16.9 **P** | 17.3 **P** | 16.3 **P** | 14.3 **P** | 15.6 **P** | 6.6 **P** |
| `gray-900` `#333333` | 12.6 **P** | 11.9 **P** | 11.3 **P** | 11.6 **P** | 10.9 **P** | 9.6 **P** | 10.4 **P** | 4.4 **G** |
| `gray-700` `#666666` | 5.7 **P** | 5.4 **P** | 5.1 **P** | 5.3 **P** | 5.0 **P** | 4.4 **G** | 4.7 **P** | 2.0 **F** |
| `gray-500` `#a3a3a3` | 2.5 **F** | 2.4 **F** | 2.3 **F** | 2.3 **F** | 2.2 **F** | 1.9 **F** | 2.1 **F** | 1.1 **F** |
| `blue-700` = `brand-deep-blue` `#1E3A5F` | 11.5 **P** | 10.8 **P** | 10.3 **P** | 10.6 **P** | 9.9 **P** | 8.7 **P** | 9.5 **P** | 4.0 **G** |
| `brand-cyan` `#25D1E0` | 1.9 **F** | 1.8 **F** | 1.7 **F** | 1.7 **F** | 1.6 **F** | 1.4 **F** | 1.5 **F** | 1.5 **F** |
| `red-700` `#e5484d` | 3.9 **G** | 3.7 **G** | 3.5 **G** | 3.6 **G** | 3.4 **G** | 3.0 **G** | 3.2 **G** | 1.4 **F** |
| `amber-700` `#b76e00` | 4.0 **G** | 3.8 **G** | 3.6 **G** | 3.7 **G** | 3.5 **G** | 3.0 **G** | 3.3 **G** | 1.4 **F** |
| `green-700` `#1f8f4d` | 4.1 **G** | 3.9 **G** | 3.7 **G** | 3.8 **G** | 3.6 **G** | 3.1 **G** | 3.4 **G** | 1.5 **F** |
| `teal-700` `#008577` | 4.5 **P** | 4.3 **G** | 4.1 **G** | 4.2 **G** | 3.9 **G** | 3.4 **G** | 3.8 **G** | 1.6 **F** |
| `purple-700` `#7c3aed` | 5.7 **P** | 5.4 **P** | 5.1 **P** | 5.2 **P** | 4.9 **P** | 4.3 **G** | 4.7 **P** | 2.0 **F** |
| `pink-700` `#db2777` | 4.6 **P** | 4.3 **G** | 4.1 **G** | 4.2 **G** | 4.0 **G** | 3.5 **G** | 3.8 **G** | 1.6 **F** |

**Usage rules confirmed by the tables (unchanged in intent, re-verified on v0.4 surfaces):**

- `brand-deep-blue` on dark chrome stays **Fail** — never deep-blue fills on dark surfaces; deep blue appears on cyan fills (6.2:1 **P**) or light surfaces only.
- `brand-cyan` on light surfaces stays **Fail** — accent/icons/active indicators only, never body text on white or light gray (§Brand Colors cyan rule).
- `gray-500` is graphical/decorative (edges, separators, tick marks) — not body text; dark values pass graphical (3.0+) except on `gray-300`, where it must not appear (same restriction as pre-v0.4).
- Light semantic `*-700` accents on white-family surfaces are markers, large text, or status dots — body-copy status text uses the `*-1000` step on its tinted fill (badge soft variants), which is unchanged by v0.4.
- **Scrim rule:** no text is set directly on the scrim — overlay surfaces (dialog, palette, popover) are opaque `background-100` above it. Light scrim effective `#999999` fails most text pairings by design; dark scrim effective `#04080D` passes all (18.4:1 primary text).
- Canvas chrome: node borders (1.4–1.8:1 effective) and grid dots (~1.1:1 effective) are whisper-subtle decorative hairlines by intent — node boundaries are carried by fill delta + `elevation-1`, not by border contrast. `canvas-edge`/`canvas-edge-hover` remain graphical lines connecting nodes whose endpoints carry the interactive affordance.

## Typography

Typography values live in frontmatter `typography:`. Use a system stack by default so the UI works without webfont fetch. If a future build bundles Geist, map to the same token names. Prioritize long-session readability over visual novelty.

Font families:

- `font-sans`: `Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif` for UI and prose.
- `font-mono`: `"SFMono-Regular", "Cascadia Code", "Roboto Mono", Consolas, monospace` for IDs, ports, code-like values, and tabular metrics.
- `font-display` (V1.121): `"Source Serif 4", Georgia, "Times New Roman", ui-serif, serif` — self-hosted OFL serif for the **content voice** (see §Design Concept). Source Serif 4 is vendored as a Latin subset (weights 400 regular + 600 semibold, `font-display: swap`); the system-serif tail of the stack renders until the webfont arrives and wherever the asset is absent.

Role intent:

- `display-*` (V1.121): content-voice titles only — work/world/chapter titles, manuscript headings, empty-state headlines on authoring surfaces, brand page. Sizes mirror the `heading-32/24/20` layout slots so swapping voices does not reflow chrome; serif metrics carry weight 600 and tightened tracking. **Never** substitute `display-*` into interface-voice positions (nav, buttons, tables, badges, labels).
- `heading-*`: page titles, view titles, card section titles, and dense section titles.
- `label-*`: form labels, nav labels, table headers, and badge labels.
- `copy-*`: primary body copy, default UI copy, and dense helper text.
- `button-*`: default and compact buttons.
- `*-mono`: IDs, schema versions, cursor values, and code-like inline values.
- `reading-prose-*`: Theme-independent reading-surface metrics (measure, line-height, paragraph-spacing) — identical values in light and dark.

Numeric columns use `font-variant-numeric: tabular-nums`.

---

## Spacing & Layout

Spacing values live in frontmatter `spacing:`. Base unit: **4px**. Prefer mechanical spacing over bespoke values.

### Rhythm

- `8px`: label + input, icon + text, badge + label.
- `16px`: related control groups and table toolbars.
- `24px`: card body padding and form section padding.
- `32–40px`: between major dashboard sections.

### Breakpoints

| Token | Width | Intent |
| --- | --- | --- |
| `sm` | `401px` | Small phones and up |
| `md` | `601px` | Large phones / narrow tablets |
| `lg` | `961px` | Desktop shell with sidebar |
| `xl` | `1200px` | Wide dashboard content |
| `2xl` | `1400px` | Dense admin displays |

### Layout Rules

- Use a fixed sidebar only at `lg` and above; collapse to top navigation below `lg`.
- Main content max width: `1200px`, with `24px` side padding on desktop and `16px` on mobile.
- Dashboard cards use 2 columns at `lg`, 3 columns only when the content remains readable.
- Tables must have horizontal overflow wrappers on narrow screens.

---

## Elevation

Hierarchy comes from borders and tonal surfaces first. Shadows are subtle, two-part (tight ambient + soft key), and only clarify layers. Values live in frontmatter `elevation:` (V1.121 normalized all five levels to the same recipe; light theme tints the shadow color toward ink blue, dark theme uses pure black with stronger alphas).

| Token | Use | Light | Dark |
| --- | --- | --- | --- |
| `elevation-0` | Flat / sunk into surface | `none` | `none` |
| `elevation-1` | Resting card / canvas node at rest | `0 1px 2px rgba(15,23,42,0.04), 0 1px 3px rgba(15,23,42,0.03)` | `0 1px 2px rgba(0,0,0,0.40), 0 1px 3px rgba(0,0,0,0.30)` |
| `elevation-2` | Hover / raised (interactive lift) | `0 2px 4px rgba(15,23,42,0.06), 0 4px 12px -2px rgba(15,23,42,0.05)` | `0 2px 4px rgba(0,0,0,0.50), 0 4px 12px -2px rgba(0,0,0,0.40)` |
| `elevation-3` | Popover / floating (menus, tooltips, command panels) | `0 1px 1px rgba(15,23,42,0.03), 0 8px 24px -12px rgba(15,23,42,0.18)` | `0 1px 1px rgba(0,0,0,0.60), 0 12px 28px -12px rgba(0,0,0,0.70)` |
| `elevation-4` | Modal / dragging | `0 1px 1px rgba(15,23,42,0.04), 0 24px 48px -24px rgba(15,23,42,0.30)` | `0 1px 1px rgba(0,0,0,0.70), 0 28px 56px -24px rgba(0,0,0,0.85)` |

**Alias chain (no consumer breakage):** the legacy names remain as keys and resolve onto the scale — `shadow-card` → `elevation-1`, `shadow-popover` → `elevation-3`, `shadow-modal` → `elevation-4`. `elevation-2` has no legacy alias; consume it directly (`shadow-elevation-2`) for hover states.

**Interactive recipes:**

| Component | Rest | Hover | Pressed / active | Dragging / selected |
| --- | --- | --- | --- | --- |
| Interactive card | `elevation-1` | `elevation-2` + `translateY(-1px)` over 160ms `ease-standard` | `elevation-1`, transform removed | — |
| Canvas node | `elevation-1` | `elevation-2` | `elevation-1` | `elevation-4` while dragging; selected keeps the two-layer ring (`canvas-node-border-selected`), not a shadow change |
| Popover / menu | `elevation-3` | — | — | — |
| Dialog / modal | `elevation-4` | — | — | — |

| Token | Light | Dark | Use |
| --- | --- | --- | --- |
| `scrim` | `rgba(0,0,0,0.40)` | `rgba(0,0,0,0.60)` | Backdrop fill behind modal/dialog/command-palette overlays (`bg-scrim`); dims the surface below. Dark value is stronger to separate the overlay from an already-low-luminance dark chrome. Re-checked against the v0.4 ink surfaces — unchanged (§Contrast (AA, recomputed)). |

---

## Motion

Motion clarifies state change; it is not decoration. Most dashboard interactions should feel instant. Values live in frontmatter `motion:`.

| Token | Value | Use |
| --- | --- | --- |
| `duration-instant` | `0ms` | Table filtering, data refresh replacement |
| `duration-state` | `120ms` | Hover/focus/pressed states |
| `duration-popover` | `160ms` | Menus, dropdowns, tooltips |
| `duration-modal` | `220ms` | Dialog open/close |
| `duration-enter` | `200ms` | Entering surfaces (popover/menu content in, toast in) |
| `duration-exit` | `140ms` | Dismissing surfaces (exit is always faster than enter) |
| `ease-standard` | `cubic-bezier(0.16, 1, 0.3, 1)` | Default UI ease |
| `ease-emphasized` | `cubic-bezier(0.2, 0.8, 0.2, 1)` | Modal/panel enter |

**Per-component recipes:**

| Component | Recipe |
| --- | --- |
| Card hover lift | `elevation-1` → `elevation-2` + `translateY(-1px)`, 160ms `ease-standard`; reverse on leave |
| Popover / menu | Enter: opacity + `scale(0.98 → 1)`, `duration-enter` `ease-standard`. Exit: opacity out, `duration-exit` |
| Dialog / modal | Enter: opacity + `translateY(4px → 0)`, `duration-modal` `ease-emphasized`; scrim fades in `duration-enter`. Exit: opacity out `duration-exit` |
| Canvas node | Rest→hover shadow swap 160ms `ease-standard`; dragging picks up `elevation-4` instantly (`duration-instant`) so the node never lags the cursor |
| Toast | Enter: slide + fade `duration-enter`; exit: fade `duration-exit` |

Always honor `prefers-reduced-motion: reduce` by dropping nonessential transform/opacity transitions — for every recipe above, reduced motion means instant state change with no transform/opacity animation.

---

## Shapes

Radius values live in frontmatter `rounded:`. Radii stay tight and utility-oriented. Do not mix very rounded and sharp corners in a single view.

---

## Focus

All interactive elements expose `:focus-visible` using the two-layer ring defined in `components.focus-ring`:

```css
box-shadow:
  0 0 0 2px var(--color-background-100),
  0 0 0 4px var(--color-blue-700);
```

On brand-filled surfaces (deep blue button), invert the inner ring to `brand-white` or use `brand-cyan` outer ring only when contrast remains ≥ 3:1.

---

## Logo Usage

Canonical SVG assets ship from `@42ch/nexus-ui/assets/logos/`. PNG sources are provenance-only (Git LFS).

| Variant | File | Surface |
| --- | --- | --- |
| Deep blue mark | `logo-primary.svg` | Light nav, sidebar, light shell header |
| Cyan mark | `logo-color.svg` | Dark nav, dark shell header |
| White mark | `logo-white.svg` | Dark hero, photography overlays, high-contrast panels |
| Monotone mark | `logo-mono.svg` | Inline UI; inherits `color` via `currentColor` |

**Rules:**

- Minimum rendered height: **24px** (`logoMinSizePx`).
- Clear space: **≥ 25%** of logo height on all sides.
- Alt text: `Nexus` on `<img>`; inline SVGs include `<title>` and `<desc>`.
- Do not recolor SVG fills outside the mono variant.
- Do not place cyan mark on white/light gray without a contrast check — prefer `logo-primary.svg` on light surfaces.

---

## Voice & Content

- Helpful, plain, local-first — a careful CLI message translated into UI copy.
- Title Case for nav, buttons, page titles, action verbs; sentence case for helpers, errors, and toasts.
- **Buttons and CTAs are Verb-only (V1.117)** — a single Title Case verb: `Save`, `Create`, `Delete` (zh-CN: `保存`, `创建`, `删除`). The changed object is usually obvious from page context; do **not** use Verb + Noun on button labels.

  | Before (Verb + Noun) | After (Verb-only) |
  | --- | --- |
  | `Save Agent` / `保存智能体` | `Save` / `保存` |
  | `Create Work` (button) | `Create` |
  | `Delete Work` (button) | `Delete` — dialog title keeps the object: `Delete Work` |

- **Boundary — these surfaces keep naming the object** and are unchanged by the Verb-only rule: page titles and dialog titles may still use Verb + Noun (`Create Work` page title; `Delete Work` dialog title); nav items stay Title Case; helpers and toasts still name the changed object. Destructive actions put the verb on the button (`Delete`) and the object in the dialog title or `aria-label` when screen readers need it.
- Prefer author-facing nouns: `Work`, `preset`, `finding`, `local daemon`.
- Toasts name the changed object; no trailing period.
- Avoid protocol jargon (`ACP`, `cursor token`) in product surfaces unless diagnostics explicitly require it.

---

## Package Exposure (`@42ch/nexus-ui`)

| Root token | Package export | CSS variable |
| --- | --- | --- |
| `brand-deep-blue` | `brandColors.deepBlue` | `--nexus-brand-deep-blue` |
| `brand-cyan` | `brandColors.cyan` | `--nexus-brand-cyan` |
| `brand-white` | `brandColors.white` | `--nexus-brand-white` |
| Logo variants | `logoVariants.*` | Import SVG path from package exports |

**Import paths (stable public API):**

- `@42ch/nexus-ui` — token constants
- `@42ch/nexus-ui/tokens` — direct token module
- `@42ch/nexus-ui/theme.css` — brand CSS custom properties
- `@42ch/nexus-ui/assets/logos/*.svg` — logo assets

---

## Component Primitives

Component token values live in frontmatter `components:`. All components must expose visible `:focus-visible` styles using a two-layer ring.

### Button

Variants and sizes: see frontmatter `components.button`. The preset `Validate` action uses `primary` when it is the main form action, or `secondary` with a `blue-700` leading icon when paired with a separate save action.

#### Button Contrast Invariant (V1.94 corrected)

> **Background decides text color, independent of light/dark mode.**
>
> - **Dark background** (deep blue, red, dark gray, saturated dark colors) → **light/white text**.
> - **Light/bright background** (cyan, light gray, pastels) → **dark text**.
>
> Practical applications:
> - Light mode primary `bg-blue-700` (dark) → `text-white` (light).
> - Dark mode primary `dark:bg-brand-cyan` (light/bright) → `dark:text-brand-deep-blue` (dark).
> - Destructive `bg-red-800` (dark) → `text-white` (light), unchanged across modes.

**Dark primary token fix (V1.94):** `dark:bg-brand-cyan dark:text-brand-deep-blue` (was `dark:text-white`).

### Input / Select / Textarea

Variants: see frontmatter `components.input-select-textarea`. Textarea min height: `96px`. Placeholder uses `gray-700`. Helper text uses `copy-13`; error helper uses `red-700`.

### Card

Default, compact, and hero/status card values: see frontmatter `components.card`.

#### Card.Title voice (`components.card.title.voice` — V1.121 contract)

`CardTitle` takes an optional `voice?: 'interface' | 'content'` prop (additive, non-breaking; default `'interface'`):

- `voice="interface"` (default): the current `text-heading-16 font-heading` sans treatment, unchanged. Use on all interface cards (settings, dialogs, table cells, dashboards).
- `voice="content"`: swaps to `font-display text-display-20 tracking-tight` — the serif content voice, sized to the same 20px slot so the card does not reflow. Use **only** on cards presenting a creative entity (work card, world card, brand-page card).

The opt-in is prop-based (not a class convention, not a CVA variant) so the recipe has exactly one implementation and call sites stay greppable (`voice="content"`). Serif discipline per §Design Concept applies — if the card does not present a creative entity, it does not get the serif.

### Table

- Header: `background-200`, `label-12`, `gray-900`, bottom border `gray-alpha-400`.
- Rows: `copy-14`, primary text `gray-1000`, secondary `gray-900`; hover `background-200`; selected `background-300`.
- Use `label-12-mono` for IDs/cursors and tabular figures for numeric columns.
- Empty table row: sentence-case helper plus first action if applicable.

### Badge / Status Pill

Tone axis: `tone=soft|solid` (API on `@42ch/nexus-ui` Badge). Default is **soft** — omitting `tone` preserves soft callers (`StatusBadge` and other wrappers need no cutover).

- **Soft (default):** tinted fill + semantic text. Borders are strengthened for light-background readability: neutral uses `gray-alpha-400`; semantic variants use ~50% alpha borders (was ~30%).
- **Solid (opt-in):** solid semantic fill + high-contrast text; `border-transparent` (no visible border). Light solid fills are locked in frontmatter `components.badge-status-pill.solid` with `#ffffff` text. Dark solid follows the Button Contrast Invariant: bright semantic fills use `brand-deep-blue` text (white fails AA); dark neutral solid uses `gray-200` + white (see `DESIGN.dark.md`).

Variant × tone values: see frontmatter `components.badge-status-pill` (nested `.soft` / `.solid`). Base: height 24px, `px-2`, `rounded-pill`, `label-12`, `font-semibold`.

### Toast

Toast values: see frontmatter `components.toast`. Variants use the semantic accent on the leading icon/bar. Toasts name the changed object; no trailing period.

### Sidebar Nav

Sidebar values: see frontmatter `components.sidebar-nav`. Collapsed/mobile nav must keep labels accessible via text, not icon-only navigation.

### Dialog / Popover / Sheet

Dialog/popover values: see frontmatter `components.dialog` and `components.popover`. Sheet (end-aligned drawer) values: see frontmatter `components.sheet`. Overlays on all three use the `scrim` color token (§Elevation — scrim convergence, V1.121).

### Tabs

**Classification (V1.106):** keep-web — owner `apps/web/src/components/ui/tabs.tsx`. Studio may reference via transitional `@web-ui/tabs` only; not package-promoted this iteration.

Token values: see frontmatter `components.tabs`.

| State | Visual | Interaction |
| --- | --- | --- |
| Default | `gray-800` label on transparent trigger inside `background-200` list | Click selects tab |
| Hover | `gray-alpha-100` fill; label `gray-1000` | Pointer only |
| Active | `background-100` fill + `shadow-card`; label `gray-1000` | Selected panel visible |
| Disabled | Label `gray-700`; `not-allowed` cursor | No selection change |
| Focus-visible | Global two-layer focus ring on trigger | Keyboard activation |

**Keyboard:** Arrow keys move between tab triggers when focus is inside the tablist; implement roving `tabindex` (active trigger `tabindex={0}`, siblings `-1`) or an equivalent documented pattern. `Enter` / `Space` activate the focused trigger.

**Voice & Content:** Tab labels use Title Case in product surfaces — examples: **Agent**, **Workspace**, **Advanced** (Settings); **Creator**, **Orchestrator** (shell).

### States

**Classification (V1.106):** keep-web — owner `apps/web/src/components/ui/states.tsx` (`Spinner`, `LoadingState`, `EmptyState`, `ErrorState`). Studio references via `@web-ui/states` when fixtured.

Token values: see frontmatter `components.states`.

| Primitive | Token use | Voice & Content examples |
| --- | --- | --- |
| `Spinner` | `components.states.spinner` — size `16px`, color `blue-700` | Icon-only; pair with text in `LoadingState` |
| `LoadingState` | `components.states.loading` — `copy-14` at `gray-700`, `space-2` gap, `space-6` vertical padding | *Scanning for local ACP agents…* (sentence case, present participle + ellipsis) |
| `EmptyState` | `components.states.empty` — title `display-24` content voice (serif, V1.121 v0.4)/`gray-1000`, description `copy-14`/`gray-900`, `space-2` gap, `space-16` vertical padding | Title: **No agents found on PATH**; helper: *Install an agent or add a custom launch command below.*; host-owned `action` slot |
| `ErrorState` | `components.states.error` — title `heading-16`/`red-1000`, description `copy-14`/`red-900`, tinted background/border (`error-surface` / `error-surface-border` tokens, V1.121), `rounded-card`, `space-4` padding; retry `label-14`/`blue-700` | Title: **Could not load this view**; helper: sentence-case transport or plain-language reason; action: **Try again** |

`EmptyState` accepts an optional `action` ReactNode — the host renders the first-step CTA (Verb-only, e.g. **Create**); the primitive does not embed routing.

`ErrorState` uses `role="alert"`; default retry label **Try again** (Title Case action).

### Form Field (composition)

**Cross-reference:** [`.mstar/iterations/v1.100/specs/form-field-contract.md`](.mstar/iterations/v1.100/specs/form-field-contract.md) (locked).

**Ownership split:**

| Layer | Owner | Responsibility |
| --- | --- | --- |
| Package (`@42ch/nexus-ui`) | `Input`, `Label`, `Textarea` | Presentational control styling; `invalid` prop → error border + `aria-invalid`; forwards native `id` / `htmlFor` |
| App / fixture | Composition | Helper text, error text, required/optional copy, `aria-describedby` wiring, validation logic, submit behavior |

**Composition order (top → bottom):**

1. `Label` with `htmlFor` matching control `id` (Title Case label text; optional *(optional)* suffix is app copy)
2. `Input` or `Textarea` with `id`, optional `invalid`, and `aria-describedby` listing helper/error element IDs
3. Helper paragraph (`copy-13`, `gray-600` / `gray-700`) — sentence case
4. Error paragraph (`copy-13`, `red-700`, `role="alert"`) — sentence case; shown when `invalid`

Package controls do **not** render helper/error/required ornaments. Settings and wizard form rows follow this stack.

### Launch & daemon status

Author-facing daemon chrome on the desktop launch → Control Room path. Token values: frontmatter `components.launch-daemon` and `components.daemon-status-indicator`. Footer/status-bar behavior: [`.mstar/specs/web-ui.md`](.mstar/specs/web-ui.md) §29.6 (running = restart icon only; degraded/error → `MainBanner`).

| Surface | State | Title (Title Case) | Helper / body (sentence case) | Primary action |
| --- | --- | --- | --- | --- |
| DaemonReadySplash | waiting | **Starting daemon…** | *This takes a few seconds on first launch.* | — |
| DaemonReadySplash | error | **Daemon not ready** | Transport or plain-language `detail` (pre-wrapped) | **Restart Nexus** |
| DaemonReadySplash | recovery | — (tertiary action row) | *This will clear the daemon's local state database (config, registry cache). Your creative files in the workspace are not affected.* | **Reset local database** (tertiary) |
| MainBanner | starting | **Daemon starting…** | *Nexus is starting the local daemon.* | **Restart Daemon** |
| MainBanner | degraded | **Daemon reconnecting** | *Nexus is retrying the local daemon connection.* | **Restart Daemon** |
| MainBanner | stopped | **Daemon stopped** | *Restart the daemon to use local workspace features.* | **Restart Daemon** |
| MainBanner | error (port) | **Port unavailable** | Server `detail` when present | **Restart Daemon** |
| MainBanner | error (generic) | **Daemon did not start** | *Nexus could not start its background service. Check the logs or try restarting.* | **Restart Daemon** |
| Status bar | running | — (icon-only restart affordance) | Cross-ref `web-ui.md` §29.6 | Restart icon button |

While loading, banner primary shows **Restarting…** (sentence case, present participle + ellipsis). Avoid protocol jargon in author-facing strings unless diagnostics explicitly require it.

**Studio (V1.106):** `/surfaces/launch` imports presentational `DaemonReadySplash`; `/surfaces/banner` uses composition-only MainBanner fixtures (no daemon IPC in Studio).

### Editor

The outline editor is a planning surface, not the body manuscript editor. It should feel closer to an intentional note/workbench than a document processor: compact toolbar, clear save state, and no hidden background writes.

Editor token values: see frontmatter `components.editor`.

| Element | Token use | Size / rhythm | States |
| --- | --- | --- | --- |
| Editor frame | `editor-surface`, `editor-border`, `rounded.card` | Min height `360px`; padding `space-6` | `:focus-within` swaps border to `editor-border-active` and uses global focus ring |
| Toolbar | `editor-surface-muted`, bottom border `editor-border` | Height `44px`; gap `space-1`; horizontal padding `space-2` | Sticky within editor panel if content scrolls |
| Toolbar button | `button-12`, `rounded.control` | `32px` square or min-width `32px` | hover `editor-toolbar-control-hover`; active `editor-toolbar-control-active` |
| Save-state indicator | `label-12`, semantic dot | Dot `8px`; gap `space-2` | `Saved` green, `Unsaved` amber, `Save failed` red; always include text, not color alone |
| Markdown helper | `copy-13`, `gray-900` | Footer padding `space-3` | Explain that body writing is read-only/deferred when relevant |

### Data Table

Chapter structure tables extend the base `Table` primitive with inline-edit and chapter-status semantics. Token values: see frontmatter `components.data-table`.

### Context Menu

Token values: see frontmatter `components.context-menu`. Actions: `Copy Path` only for body/outline path affordances.

---

## Setup Wizard Surface (V1.96 — Level 3 Production; V1.105 portrait amend)

The setup wizard is the user's first interaction with Nexus. V1.96 introduced a centered single-card surface with a left step rail. **V1.105 (P2)** reshapes the card to a fixed portrait geometry with top horizontal steps. All token values live in frontmatter `components.setup-wizard-surface` and `components.setup-wizard-step`. The dark theme shares the same token names with dark-tuned values in [`DESIGN.dark.md`](DESIGN.dark.md).

### Layout shell (V1.105)

- Portrait card: **480px** max width (`wizard-max-width` → `max-w-setup-wizard-step-wizard-max-width`), **min(720px, 85vh)** height (`wizard-max-height` → `h-setup-wizard-wizard-max-height` + `max-h-[85vh]`).
- **Top horizontal** step indicator (`TopStepIndicator`) above scrollable step content — labels **Agent** / **Workspace** / **Done** (`agent` / `workspace` / `done`); states `complete` / `active` / `pending` reuse `setup-wizard-step-circle-*` and `setup-wizard-step-label-*`; horizontal `flex` row with optional short connectors (`setup-wizard-step-connector` color).
- Step body: `flex-1 min-h-0 overflow-y-auto`; primary CTA stays bottom-anchored (`mt-auto` on `data-testid="wizard-cta-row"`).
- Left step rail (`step-panel-width` 208px / `w-setup-wizard-surface-step-panel-width`) **retired** for wizard chrome — do not reference `step-panel-*` or vertical connectors in P2 wizard layout.

### Layout shell (V1.96 legacy — superseded by V1.105 for wizard)

- The wizard card is centered in the viewport both horizontally and vertically.
- The step indicator list (left panel) and the current step's content area (right panel) live inside **one shared card chrome** container.

### Inline input row pattern (Workspace step)

The workspace location affordance is one tightly-coupled inline row (V1.105 step 2 — Workspace; historically step 1 Welcome):

- Folder icon (`input-row-icon-color`) + label (`input-row-label-color`) + path text (`input-row-path-color`) + Browse button.
- The entire row uses `input-row-bg` with `input-row-border` and `input-row-rounded`.
- Minimum row height: `48px` (`input-row-min-height`).

### Primary CTA

The Continue/Finish button has max-width `cta-primary-max-width`. Back button renders as a smaller secondary button adjacent, with `cta-container-gap` spacing.

### Done step copy (V1.106 P1)

Portrait Done step centers the success stack in `data-testid="wizard-step-body"`; CTA row stays bottom-anchored.

| Element | Copy pattern | Example |
| --- | --- | --- |
| Heading | Title Case title + celebratory emoji **after** title text | **You're ready 🎉** |
| Helper | One line, sentence case | *Open Nexus to start writing. You can change settings anytime.* |
| Primary CTA | Verb-only (Title Case verb) | **Open** |
| Finishing state | Present participle + ellipsis | *Finishing…* |

---

## Author Reflection — Reading Surface (V1.79+)

Reading-surface tokens (`reading-prose-*`, `reading-chapter-nav`, `reading-progress-indicator`, `reading-maturation-badge`) define the prose column shape and chapter navigation chrome. All token values are in the frontmatter. Theme-independent metrics (`reading-prose-measure`, `reading-prose-line-height`, `reading-prose-paragraph-spacing`) keep the prose column shape stable across light/dark.

### Reading Chrome (V1.91; v0.4 amend V1.121)

Four work-profile-specific reading rendering tokens define profile-differentiated typography for read-only prose rendering. Token values in frontmatter `components.reading-chrome-novel/essay/game-bible/script`.

**v0.4 amend:** the novel chapter title is the canonical content-voice consumer — its family now resolves through `typography.font-display` (weight 600, matching the vendored serif subset ramp; the CSS block's hardcoded `Georgia` stack is absorbed). The game-bible callout/badge backgrounds resolve as `color-mix` semantic tints on `teal-700`/`amber-700` instead of raw rgba. Reading chrome is a **content-voice** surface by definition (§Design Concept); profile headings that are interface chrome (essay section-heading) stay sans.

### Annotations (V1.89)

Four-color annotation highlight system + inspector/selection-toolbar chrome. Token values in frontmatter `components.reading-annotation-highlight-*`, `components.reading-annotation-inspector`, `components.reading-selection-toolbar`.

---

## SOUL Personality Visualization (V1.79–V1.81)

Keyword-cluster (network of nodes sized by frequency) and temporal-drift (stacked-band timeline) tokens define the Creator SOUL visualization surface. Token values in frontmatter `components.soul-viz-*` and `components.soul-narrative-prose` / `components.soul-growth-curve-stroke`.

---

## Canvas Surface (V1.70, V1.73; V1.121 v0.4)

Infinite-canvas graph primitives and World KB entity-card / promotion / relationship tokens define the canvas workspace. Token values in frontmatter `components.canvas.*`.

The canvas is the product's signature surface and follows §Design Concept strictly:

- **Ambient.** The canvas field is an atmospheric surface, not a mode flip: ink in dark (`canvas-surface` on the `background-200/300` ink band), warm paper in light. The dot grid is decorative whisper-texture (`canvas-grid` alpha + `canvas-grid-gap` 20px + `canvas-grid-dot-size` 1.5px) — never strong enough to read as instrumentation. Minimap and controls inherit node fills/borders; node chrome carries `elevation` per §Elevation (rest `elevation-1`, hover `elevation-2`, dragging `elevation-4`).
- **Chromatic hygiene.** Every status/marker/edge color resolves to the brand semantic scales (`blue-700`, `green-700`, `amber-700`, `red-700`, `teal-700`, `purple-700`, `pink-700`, `gray-500`) — Tailwind-palette leftovers were remapped hue-preserving in v0.4. The normative mapping table lives in §Appendix: Canvas Chromatic Hygiene Mapping. Per-surface accent spines stay semantic: strategy = `purple-700`, outline = `amber-700`, World KB = `teal-700`.
- **Node widths.** `components.canvas.node-width.<role>` fixes the five node width slots (`strategy-root` 260px, `strategy-primary` 140px, `strategy-secondary` 150px, `outline-scene-beat` 160px, `default` 176px) so node geometry is a design decision, not a per-component magic number.

---

## Footer Profile Switcher (V1.94)

Sidebar footer avatar row tokens: see frontmatter `components.footer-profile`.

---

## Creator Memory (V1.79–V1.81)

Review-loop pending-count badge, task-kind chips, fragment browser chrome, and inspector tokens. Token values in frontmatter `components.memory-*`.

---

## Findings Remediation

6-state finding-status badges + triage chrome: see frontmatter `components.finding-status-pill` and `components.finding-triage`.

---

## Implementation Mapping

CSS variable tokens are projected from the frontmatter into `tooling/design-tokens/src/tokens.css`. Tailwind theme extensions live in `tooling/design-tokens/tailwind.preset.ts`. Both `apps/web` and `apps/design-studio` import the shared CSS pipeline via `@nexus/design-tokens/tokens.css` + the Tailwind preset.

**CSS variable naming convention:** `--color-<token-name>` (hyphenated). Example: `colors.blue-700` → `--color-blue-700`. Structural tokens (spacing, typography sizes) use `--<category>-<name>`.

**Theme toggle:** Tailwind `class` strategy — `.dark` class on `<html>` swaps color + shadow CSS variables; token names are identical in both themes.

**File references (post-V1.98 unification):**

- Design SSOT: [`DESIGN.md`](DESIGN.md) / [`DESIGN.dark.md`](DESIGN.dark.md) (repo root — this file)
- CSS vars + theme layers: `tooling/design-tokens/src/tokens.css`
- Tailwind preset: `tooling/design-tokens/tailwind.preset.ts`
- Brand package: `@42ch/nexus-ui/theme.css`

**Blue-* alias policy:** `blue-700` remains the web primary interactive token name in light theme (maps to `brand-deep-blue`). In dark theme, `blue-700` maps to `brand-cyan`. Component tokens using `{colors.blue-700}` continue to resolve correctly in both themes. Do **not** rename `blue-*` to `brand-deep-blue` in CSS vars.

---

## Intentional Drift Register (V1.98 merge)

| Token | Pre-merge (apps/web) | Post-merge (root) | Reason |
| --- | --- | --- | --- |
| *(none)* | — | — | apps/web neutrals/accent values preserved verbatim; no value changes |

---

## Appendix: Canvas Chromatic Hygiene Mapping (V1.121, normative)

Every Tailwind-palette leftover in `components.canvas.*` was remapped **hue-preserving** onto the brand semantic scales in v0.4. This table is the normative record of that mapping, applied verbatim to both DESIGN files; semantic *meaning* of every token (status, promotion state, confidence, edge kind) is unchanged — only the pigment moves on-palette. Post-v0.4, no `components.canvas.*` value may reference a Tailwind-palette hex (`#94A3B8`, `#3B82F6`, `#10B981`, `#F59E0B`, `#A78BFA`, `#0EA5E9`, `#EF4444`, `#8B5CF6`, `#EDE9FE` — grep-enforced).

| Token | v0.3 light | v0.4 light | v0.3 dark | v0.4 dark | Hue family |
| --- | --- | --- | --- | --- | --- |
| `canvas-outline-chapter-card-status-pending` | `#94A3B8` | `gray-500` `#a3a3a3` | `#64748B` | `gray-500` `#737373` | slate → neutral gray |
| `canvas-outline-chapter-card-status-drafted` | `#3B82F6` | `blue-700` | `#60A5FA` | `blue-700` | blue → brand blue (dark: brand-cyan) |
| `canvas-outline-chapter-card-status-completed` | `#10B981` | `green-700` | `#34D399` | `green-700` | emerald → green |
| `canvas-outline-timeline-event-pin` | `#F59E0B` | `amber-700` | `#FBBF24` | `amber-700` | amber |
| `canvas-outline-foreshadow-edge` | `#A78BFA` | `purple-700` | `#C4B5FD` | `purple-700` | violet → purple |
| `canvas-outline-timeline-marker` | `#0EA5E9` | `teal-700` | `#38BDF8` | `teal-700` | sky → teal |
| `canvas-outline-conflict-marker` | `#EF4444` | `red-700` | `#F87171` | `red-700` | red |
| `canvas-outline-scene-status-drafted` | `#3B82F6` | `blue-700` | `#60A5FA` | `blue-700` | blue |
| `canvas-outline-scene-status-completed` | `#10B981` | `green-700` | `#34D399` | `green-700` | green |
| `canvas-worldkb-promotion-pending` | `#F59E0B` | `amber-700` | `#FBBF24` | `amber-700` | amber |
| `canvas-worldkb-promotion-confirmed` | `#10B981` | `green-700` | `#34D399` | `green-700` | green |
| `canvas-worldkb-promotion-rejected` | `#EF4444` | `red-700` | `#F87171` | `red-700` | red |
| `canvas-worldkb-promotion-merged` | `#8B5CF6` | `purple-700` | `#A78BFA` | `purple-700` | violet → purple |
| `canvas-worldkb-source-anchor-edge` | `#A78BFA` | `purple-700` | `#C4B5FD` | `purple-700` | violet → purple |
| `canvas-worldkb-source-anchor-node` | `#EDE9FE` | `color-mix(purple-700 10%)` | `#2A2440` | `color-mix(purple-700 14%)` | purple alpha wash |
| `canvas-worldkb-computable-badge` | `#0EA5E9` | `teal-700` | `#38BDF8` | `teal-700` | sky → teal |
| `canvas-worldkb-conflict-marker` | `#EF4444` | `red-700` | `#F87171` | `red-700` | red |
| `canvas-worldkb-conflict-marker-fill` | `rgba(239,68,68,0.10)` | `color-mix(red-700 10%)` | `rgba(248,113,113,0.12)` | `color-mix(red-700 12%)` | red alpha wash |
| `canvas-worldkb-relationship-edge` | `#94A3B8` | `gray-500` | `#64748B` | `gray-500` | slate → neutral gray |
| `canvas-worldkb-relationship-edge-default` | `#94A3B8` | `gray-500` | `#64748B` | `gray-500` | slate → neutral gray |
| `canvas-worldkb-relationship-edge-symmetric` | `#8B5CF6` | `purple-700` | `#A78BFA` | `purple-700` | violet → purple |
| `canvas-worldkb-relationship-edge-custom` | `#DB2777` | `pink-700` (exact value) | `#F472B6` | `pink-700` | pink |
| `canvas-worldkb-relationship-confidence-low` | `#E5484D` | `red-700` (exact value) | `#FF6B6B` | `red-700` (exact value) | red |
| `canvas-worldkb-relationship-confidence-mid` | `#B76E00` | `amber-700` (exact value) | `#FFC043` | `amber-700` (exact value) | amber |
| `canvas-worldkb-relationship-confidence-high` | `#1F8F4D` | `green-700` (exact value) | `#54D58A` | `green-700` (exact value) | green |
| `canvas-worldkb-relationship-grounded-badge` | `rgba(0,107,255,0.12)` | `color-mix(blue-700 12%)` | `rgba(82,168,255,0.14)` | `color-mix(blue-700 14%)` | blue → brand blue scale (drift alignment — matches the value `tokens.css` already shipped) |
| `canvas-worldkb-entity-card-fill-selected` | `#EBF2FF` (unchanged) | `#EBF2FF` | `rgba(82,168,255,0.14)` | `color-mix(blue-700 14%)` | blue → brand blue scale (dark drift alignment) |

**Ambient alignment (same pass):** `canvas-surface` `#ebebeb` → `#EBE9E5` warm paper (light) / `#141414` → `#101D2E` ink (dark); `canvas-node-fill` dark `#1a1a1a` → `background-300`; `canvas-node-fill-hover` `#f5f5f5` → `background-300` (light) / `#2a2a2a` → `gray-200` (dark); `canvas-worldkb-entity-card-fill-default/hover` and `canvas-worldkb-relationship-inspector-fill` (dark) likewise resolve onto the ink scale (`background-300` / `gray-200`). New ambient keys: `canvas-grid-gap` `20px`, `canvas-grid-dot-size` `1.5px`, and the `components.canvas.node-width.*` family (see §Canvas Surface).
