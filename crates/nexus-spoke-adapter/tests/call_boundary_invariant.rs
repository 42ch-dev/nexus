//! # Call-boundary invariant static check (spec §7 enforcement)
//!
//! Regression guard proving the adapter crate does NOT reimplement any
//! [`spoke_operations`] lifecycle invariant. It reads the adapter source text
//! at compile time via `include_str!` and asserts four structural invariants
//! derived from the architect's refinement of tracked spec
//! `spoke-adapter-architecture.md` §7. If a future change reintroduces a
//! thick layer (control-flow branching on spoke lifecycle states, direct
//! `SpokeReject` construction, lifecycle calls outside the delegation module,
//! or drift in the sole allowlisted data-shape adaptation), this test fails
//! loudly with a clear message.
//!
//! ## The four invariants
//!
//! 1. **`ops.rs` is a thin-wrapper shape** — no control-flow keyword
//!    (`match` / `if` / `for` / `while` / `loop`) starts a trimmed
//!    non-comment line. Wrappers pass operands straight through to the
//!    underlying `spoke_operations` function; branching on spoke lifecycle
//!    state belongs in `spoke-operations`, never here.
//! 2. **`extensions.rs` calls no `spoke_operations` lifecycle function** —
//!    a `spoke_operations::<ident>(` call pattern is forbidden. Wire-type
//!    imports (`use spoke_operations::ExtensionMap;`) ARE permitted because
//!    `ExtensionMap` has no `spoke-schemas` equivalent and is a type, not an
//!    operation. `src/lib.rs` is exempt (it owns Surface B re-exports + prose
//!    doc-comments that may mention `spoke_operations`).
//! 3. **No direct `SpokeReject` construction in `src/`** — wrappers return
//!    `SpokeResult` verbatim and never assemble a `SpokeReject { ... }`
//!    struct literal themselves.
//! 4. **`build_assemble_packet` allowlist** — the one non-pass-through
//!    statement permitted in `ops.rs` is the data-shape adaptation
//!    `let wrapped: Vec<KnowledgeEntryForAssemble> = ...`. Spec §7.2 exposes
//!    `&[KnowledgeEntry]`; spoke's real API takes
//!    `&[KnowledgeEntryForAssemble]`. The wrapper bridges that with a single
//!    `let` binding + method chain (no control-flow), so invariant 1 admits
//!    it naturally. This test pins the allowlisted form by name so that drift
//!    (rename, removal, or silent widening) is caught.

/// Source text of `src/ops.rs`, embedded at compile time for static checks.
static OPS_SRC: &str = include_str!("../src/ops.rs");

/// Source text of `src/extensions.rs`, embedded at compile time for static
/// checks.
static EXTENSIONS_SRC: &str = include_str!("../src/extensions.rs");

/// Needle prefix used by [`count_spoke_operations_calls`]; kept as a constant
/// so the matcher and any future audit share one source of truth.
const SPOKE_OPERATIONS_PREFIX: &str = "spoke_operations::";

/// Count matches of the regex `spoke_operations::[a-z_][a-z0-9_]*\s*\(` in
/// `text`.
///
/// This matches a `spoke_operations` function CALL — the identifier must
/// start with `[a-z_]` (lowercase or underscore), continue with
/// `[a-z0-9_]*`, be followed by optional ASCII whitespace, and then an
/// opening paren `(`.
///
/// # What this deliberately does NOT match
///
/// A bare type import such as `use spoke_operations::ExtensionMap;` is
/// allowed through, for two independent reasons that each suffice on their
/// own:
///
/// 1. `ExtensionMap` starts with uppercase `E`, which fails the `[a-z_]`
///    first-char class.
/// 2. The identifier is followed by `;`, not `(`, which fails the trailing
///    `(` requirement.
///
/// Implementing the regex manually (rather than pulling in the `regex`
/// crate) keeps the test dependency-free; the scanned pattern is narrow and
/// fixed.
fn count_spoke_operations_calls(text: &str) -> usize {
    let mut count = 0usize;
    let mut from = 0usize;
    while let Some(relative) = text[from..].find(SPOKE_OPERATIONS_PREFIX) {
        let after_prefix = from + relative + SPOKE_OPERATIONS_PREFIX.len();
        // Advance the search cursor past this occurrence regardless of
        // whether the tail matches — the prefix cannot overlap itself, so
        // there is no risk of double-counting or looping forever.
        from = after_prefix;

        let tail = &text[after_prefix..];
        let tail_bytes = tail.as_bytes();

        // First char of the identifier must be [a-z_].
        let Some(&first) = tail_bytes.first() else {
            continue;
        };
        if !(first.is_ascii_lowercase() || first == b'_') {
            continue;
        }

        // Consume the rest of the identifier: [a-z0-9_]*.
        let mut cursor = 1usize;
        while cursor < tail_bytes.len() {
            let c = tail_bytes[cursor];
            if c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_' {
                cursor += 1;
            } else {
                break;
            }
        }

        // Consume optional ASCII whitespace (\s* restricted to ASCII, which
        // is all Rust source whitespace here).
        let mut after_ws = cursor;
        while after_ws < tail_bytes.len() && tail_bytes[after_ws].is_ascii_whitespace() {
            after_ws += 1;
        }

        // A trailing `(` makes this a call site.
        if after_ws < tail_bytes.len() && tail_bytes[after_ws] == b'(' {
            count += 1;
        }
    }
    count
}

/// Return `true` when `line` (already trimmed) begins with a Rust
/// control-flow keyword (`match`, `if`, `for`, `while`, `loop`) followed by
/// a word boundary — any byte that is neither ASCII alphanumeric nor `_`.
///
/// Equivalent to the regex
/// `^(match|if|for|while|loop)[^A-Za-z0-9_]` evaluated on the trimmed line.
/// A line that is exactly the bare keyword (nothing after) also counts, so
/// the matcher does not miss degenerate `loop` / `match` forms.
fn starts_with_control_flow_keyword(line: &str) -> bool {
    const KEYWORDS: [&str; 5] = ["match", "if", "for", "while", "loop"];
    for keyword in KEYWORDS {
        let Some(rest) = line.strip_prefix(keyword) else {
            continue;
        };
        let at_boundary = match rest.as_bytes().first() {
            None => true,
            Some(&c) => !(c.is_ascii_alphanumeric() || c == b'_'),
        };
        if at_boundary {
            return true;
        }
    }
    false
}

/// Assert invariant 1: `src/ops.rs` performs no control-flow branching.
///
/// Each public wrapper must be a single-expression pass-through; branching on
/// spoke lifecycle states (e.g. `match reject.code { SpokeRejectCode::… => … }`)
/// belongs in `spoke-operations`, not in this adapter.
#[test]
fn assertion_1_ops_rs_has_no_control_flow_branching() {
    let offenders: Vec<&str> = OPS_SRC
        .lines()
        .map(str::trim_start)
        .filter(|line| !line.starts_with("//"))
        .filter(|line| starts_with_control_flow_keyword(line))
        .collect();

    assert!(
        offenders.is_empty(),
        "src/ops.rs must not branch on spoke lifecycle states inside the adapter \
         (spec §7 call-boundary invariant). Found control-flow keyword(s) starting \
         these lines: {offenders:?}"
    );
}

/// Assert invariant 2: `src/extensions.rs` calls no `spoke_operations`
/// lifecycle function.
///
/// Wire-type imports (`use spoke_operations::ExtensionMap;`) are explicitly
/// permitted — `ExtensionMap` has no `spoke-schemas` equivalent and is a
/// type, not an operation. `src/lib.rs` is exempt from this check because it
/// owns Surface B re-exports and `//!` doc-comment prose that may mention
/// `spoke_operations`.
#[test]
fn assertion_2_extensions_rs_calls_no_spoke_operations_lifecycle() {
    let call_sites = count_spoke_operations_calls(EXTENSIONS_SRC);

    assert_eq!(
        call_sites, 0,
        "src/extensions.rs must not call spoke_operations lifecycle functions \
         (spec §7 call-boundary invariant). The wire-type import \
         `use spoke_operations::ExtensionMap;` is allowed; a call pattern \
         `spoke_operations::<lowercase_ident>(` is not. Found {call_sites} \
         call site(s)."
    );
}

/// Assert invariant 3: neither `src/ops.rs` nor `src/extensions.rs` ever
/// constructs a `SpokeReject` struct literal.
///
/// Wrappers return `SpokeResult` verbatim; reject construction is a
/// `spoke-operations` responsibility. `src/lib.rs` only re-exports the
/// `SpokeReject` *type* (`pub use … SpokeReject …`), which contains no
/// `SpokeReject {` struct literal and is therefore not scanned here.
#[test]
fn assertion_3_src_never_constructs_spoke_reject() {
    assert!(
        !OPS_SRC.contains("SpokeReject {"),
        "src/ops.rs must not construct SpokeReject {{ ... }} directly — \
         wrappers pass SpokeResult through verbatim (spec §7 call-boundary \
         invariant)."
    );
    assert!(
        !EXTENSIONS_SRC.contains("SpokeReject {"),
        "src/extensions.rs must not construct SpokeReject {{ ... }} directly \
         (spec §7 call-boundary invariant)."
    );
}

/// Assert invariant 4: the `build_assemble_packet` allowlist.
///
/// `build_assemble_packet` is the only wrapper permitted to do more than
/// pass operands through, because spec §7.2 exposes `&[KnowledgeEntry]`
/// while spoke's real API takes `&[KnowledgeEntryForAssemble]`. The bridging
/// statement is pinned here by name:
///
/// ```text
/// let wrapped: Vec<KnowledgeEntryForAssemble> = entries
///     .iter()
///     .cloned()
///     .map(KnowledgeEntryForAssemble::from_entry)
///     .collect();
/// ```
///
/// That `let` binding + method chain contains no control-flow keyword, so
/// invariant 1 admits it naturally — this test exists to document the
/// allowlist explicitly and to catch silent drift (rename, removal, or
/// widening into a second adaptation).
#[test]
fn assertion_4_build_assemble_packet_allowlist() {
    assert!(
        OPS_SRC.contains("pub fn build_assemble_packet("),
        "Allowlist anchor missing: the `build_assemble_packet` wrapper must \
         exist in src/ops.rs. If it was renamed or removed, update this test \
         and the spec §7.2 surface note together."
    );
    assert!(
        OPS_SRC.contains("let wrapped: Vec<KnowledgeEntryForAssemble>"),
        "Allowlist drift: `build_assemble_packet` must bridge entries via the \
         single statement `let wrapped: Vec<KnowledgeEntryForAssemble> = …`. \
         This is the only non-pass-through adaptation permitted in src/ops.rs \
         (spec §7.2). If the bridge changed shape, update this assertion and \
         the spec §7.2 note in lockstep."
    );
}
