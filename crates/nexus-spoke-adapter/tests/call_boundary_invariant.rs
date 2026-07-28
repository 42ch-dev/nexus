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
//!    (`match` / `if` / `for` / `while` / `loop`) appears anywhere on a
//!    non-comment line at an identifier word boundary. Wrappers pass
//!    operands straight through to the underlying `spoke_operations`
//!    function; branching on spoke lifecycle state belongs in
//!    `spoke-operations`, never here.
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

/// Return `true` when `line` contains a Rust control-flow keyword
/// (`match`, `if`, `for`, `while`, `loop`) anywhere on the line, provided
/// the keyword sits at an identifier word boundary on BOTH sides.
///
/// A byte counts as part of an identifier when it is ASCII alphanumeric or
/// `_`. The keyword therefore must be preceded by either the start of the
/// line or a non-identifier byte, AND followed by either the end of the
/// line or a non-identifier byte. Conceptually this is the regex
/// `\b(match|if|for|while|loop)\b`, implemented as manual byte-scanning to
/// keep the test dependency-free (matching the file's existing style).
///
/// Both-side boundary matters: substrings like `matcher`, `formation`,
/// `loufer`, or `for_each` do NOT trigger the helper because the keyword is
/// glued to identifier bytes on one side or the other. The forms
/// `let result = match x { … }`, `return if cond { … }`, or a bare
/// `loop { … }` all DO trigger it, regardless of where on the line they
/// sit — which is the whole point of scanning the whole line rather than
/// only the trimmed start.
///
/// # Caveats (acceptable for the current `src/ops.rs`)
///
/// This is a regression guard, not a Rust tokenizer. Two simplifications
/// are intentional and documented here so they are not a surprise:
///
/// * **Inline trailing comments** — a `//` comment tail appended to a code
///   line (e.g. `let x = …; // see match arm`) would false-positive if the
///   comment text contained a keyword. The current `src/ops.rs` has no
///   inline trailing comments at all (the wrappers are pure
///   pass-throughs), so this is a non-issue today.
/// * **String literals** — a string literal containing one of the keywords
///   (e.g. `"loop ended"`) would false-positive. The current `src/ops.rs`
///   contains no such literals.
///
/// If a future change introduces either pattern, replace this helper with
/// a proper Rust tokenizer (e.g. `syn` or `rustc_lexer`) instead of
/// extending the manual scanner.
fn contains_control_flow_keyword_at_boundary(line: &str) -> bool {
    const KEYWORDS: [&str; 5] = ["match", "if", "for", "while", "loop"];
    let bytes = line.as_bytes();

    // Identifier byte test: ASCII alphanumeric or `_`. Inlined (rather than
    // extracted to a shared helper) to match the existing style in
    // [`count_spoke_operations_calls`] and keep the matcher self-contained.
    let is_ident_byte = |b: u8| b.is_ascii_alphanumeric() || b == b'_';

    for keyword in KEYWORDS {
        let mut from = 0usize;
        while let Some(relative) = line[from..].find(keyword) {
            let start = from + relative;
            let end = start + keyword.len();
            // Advance the search cursor past this occurrence so progress is
            // guaranteed (each keyword is non-empty) and overlapping
            // matches are not double-counted.
            from = end;

            let left_boundary_ok = start == 0 || !is_ident_byte(bytes[start - 1]);
            let right_boundary_ok = end == bytes.len() || !is_ident_byte(bytes[end]);

            if left_boundary_ok && right_boundary_ok {
                return true;
            }
        }
    }
    false
}

/// Direct exercise of [`contains_control_flow_keyword_at_boundary`] to pin
/// its both-side word-boundary semantics. The static-check regression guard
/// is only useful if the matcher itself is correct, so this proves the
/// positive cases (keyword anywhere on the line, including mid-statement)
/// AND the negative cases (keyword glued to identifier bytes) in one
/// place. A future change that reverts the matcher to start-only checking,
/// or that drops the left/right boundary test, fails here loudly.
#[test]
fn helper_contains_control_flow_keyword_at_boundary_semantics() {
    // Positive: keyword at the start of the line.
    assert!(contains_control_flow_keyword_at_boundary(
        "match x { _ => () }"
    ));
    assert!(contains_control_flow_keyword_at_boundary(
        "if cond { 1 } else { 2 }"
    ));
    assert!(contains_control_flow_keyword_at_boundary(
        "for item in items {}"
    ));
    assert!(contains_control_flow_keyword_at_boundary(
        "while running.len() > 0 {}"
    ));
    assert!(contains_control_flow_keyword_at_boundary("loop { break; }"));

    // Positive: keyword ANYWHERE on the line — the case the previous
    // start-only matcher missed (Greptile P2 on PR #185).
    assert!(contains_control_flow_keyword_at_boundary(
        "let result = match x { _ => () };"
    ));
    assert!(contains_control_flow_keyword_at_boundary(
        "return if cond { 1 } else { 2 };"
    ));
    assert!(contains_control_flow_keyword_at_boundary(
        "let x = f(); match x { _ => () }"
    ));

    // Negative: keyword glued to identifier bytes on one side.
    assert!(!contains_control_flow_keyword_at_boundary(
        "let matcher = build();"
    ));
    assert!(!contains_control_flow_keyword_at_boundary(
        "let formation = vec![];"
    ));
    assert!(!contains_control_flow_keyword_at_boundary(
        "let loufer = 42;"
    ));
    assert!(!contains_control_flow_keyword_at_boundary(
        "items.for_each(|i| take(i));"
    ));
    // `for_each`: the `.` before `for` passes the left boundary, but the
    // `_` after `for` is an identifier byte, so the right boundary fails
    // and the helper correctly does NOT trigger.

    // Negative: keyword absent entirely.
    assert!(!contains_control_flow_keyword_at_boundary("let x = 1 + 2;"));
    assert!(!contains_control_flow_keyword_at_boundary(
        "fn wrapper() { validate(x) }"
    ));
}

/// Assert invariant 1: `src/ops.rs` performs no control-flow branching.
///
/// Each public wrapper must be a single-expression pass-through; branching on
/// spoke lifecycle states (e.g. `match reject.code { SpokeRejectCode::… => … }`)
/// belongs in `spoke-operations`, not in this adapter. The matcher scans the
/// whole line (not just the trimmed start) so mid-statement forms like
/// `let result = match …` or `return if …` are also caught.
#[test]
fn assertion_1_ops_rs_has_no_control_flow_branching() {
    let offenders: Vec<&str> = OPS_SRC
        .lines()
        .map(str::trim_start)
        .filter(|line| !line.starts_with("//"))
        .filter(|line| contains_control_flow_keyword_at_boundary(line))
        .collect();

    assert!(
        offenders.is_empty(),
        "src/ops.rs must not branch on spoke lifecycle states inside the adapter \
         (spec §7 call-boundary invariant). Found control-flow keyword(s) on \
         these lines (matched anywhere on the line at a word boundary): \
         {offenders:?}"
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
