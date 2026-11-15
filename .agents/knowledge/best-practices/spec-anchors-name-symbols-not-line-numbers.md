# Spec anchors name symbols, not line numbers

**Problem.** A spec sentence that says "`apply.rs:557` compares the identity" is load-bearing prose attached to a
fragile index. Any insertion above the line, any reformat, and the citation points at a different statement while
the sentence still reads as proven. In one iteration this happened twice: a `path:line` table was written, an
unrelated fix commit moved eight of its references, and a QC seat had to reconstruct the correct mapping from the
diff before the section could be trusted again.

**Pattern.** Make the **symbol** authoritative and the line a convenience:

```markdown
**Anchors.** Every reference below names the file and the item that decides the claim — function, match arm,
struct field or table row — and that name is authoritative; a `:line` only locates the item in the current tree,
so formatting may move it while the claim stays with the name.
```

Then write `resolve_applied_document` (`applied_documents.rs`), not `applied_documents.rs:41`. Keep a `:line` only
where a reader genuinely benefits from jumping, and never in a sentence whose truth depends on *that* line.

**Where this is hardest.** Tables. A verification table wants a per-row anchor, and the row's meaning is often
"this exact expression writes `applied: None`". The compromise that held: name the expression and the enclosing
function in the row, cite the line as a locator, and state the convention once at the top of the section.

**The trap underneath.** A line number is a proxy for "I read this". When the code moves, the number silently
becomes a proxy for "someone read this once". If a claim cannot be re-verified from the symbol alone, the claim is
too weak to survive a refactor — rewrite it around the behaviour, not the location.

**Related failure.** The same iteration produced a citation to a test that "proves" a persisted field is `None`
when the test only asserts the descriptor exists. A reader who trusts the anchor never learns the field was never
checked. Symbol-level naming makes that kind of overreach visible: if you cannot name the assertion, you do not
have one.
