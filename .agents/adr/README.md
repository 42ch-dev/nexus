# Architecture decision records

Most records here are one decision this repository made **among real alternatives**, filed so a reader finds it without an index: the bucket gives the fate, the class the domain, the date sorts the rest. The `archived/` bucket also preserves frozen migration history — removed-line ledgers and redirect notes carried over from the retired tracked specs corpus; each states that in its own header and none is current authority.

## When a record is required

Write one when all three hold:

1. A decision was made or is proposed among real alternatives — something was rejected, or accepted at a cost.
2. Its rationale cannot be reconstructed from the code, tests, or existing docs — "we rejected option B, at the cost of X, and would reconsider if Y" has no other home.
3. It must outlive the plan or PR that produced it — a future contributor has to confront it before reintroducing the bypass.

Exempt: mechanical edits, local edits whose whole rationale is visible in the code or PR description, frozen migration-history ledgers under `archived/` (retained only to preserve removed-text history), and decisions already recorded — update or supersede the existing record, never write a second record for the same decision.

A record is the history of a change, not a description of the system it left behind: it states the decision and why the alternatives lost. The current state that decision produced belongs to [`docs/specs/`](../../docs/specs), which the same PR keeps true to the code.

## Directory shape

```
.agents/adr/{bucket}/{class}/yyyy-mm-dd-topic.md
```

- **Bucket** — the fate: `proposed/`, `implemented/`, `rejected/`, `archived/`.
- **Class** — the domain: `feature/`, `bug-fix/`, `simplification/`, `architecture/`, `process/`, `testing/`.
- **Filename** — the record's **original** date plus a lowercase kebab topic. It never changes across a lifecycle move; only the bucket directory moves.

## Header and skeletons

The first line is `# ADR: <title>`. The header block also carries one bare line `Status: <state> — <why>`, whose state must equal the containing bucket for `proposed/`, `implemented/` and `rejected/`; an archived record keeps `Status: implemented — <why>` and carries an `Archived: YYYY-MM-DD — <reason>` line instead. `Date:`, `Scope:`, `Supersedes:` / `Superseded-by:` and `Source:` are free-form.

- [`TEMPLATE-proposed.md`](TEMPLATE-proposed.md) — `Problem`, `Proposal`, `Alternatives considered`, `Acceptance criteria`, `Risks`.
- [`TEMPLATE-implemented.md`](TEMPLATE-implemented.md) — `Problem`, `Decision`, `Alternatives considered`, `Consequences`. `Decision` is present tense.
- `rejected/` reuses the proposed skeleton with a frozen body; only `Problem`, `Proposal` and `Alternatives considered` are required.
- `archived/` keeps the implemented record's sections and is frozen after the move. Migration-history ledgers carried over from the retired specs corpus are frozen history, not decisions; they keep their `Problem` / `Decision` / `Alternatives considered` / `Consequences` headings but are never cited as current authority.

`Proposal`, `Plan`, `Migration plan` and `Acceptance criteria` are banned as sections of an implemented record. Extra sections that are neither required nor banned stay.

## Lifecycle

Three moves exist, each landed as one change — move, rewrite and header together:

- **`proposed → implemented`** — rewrite `Proposal` as a present-tense `Decision`, fold `Acceptance criteria` and `Risks` into `Consequences`, delete plan text, rewrite the Status line.
- **`proposed → rejected`** — freeze the body, put the reason in the Status line.
- **`implemented → archived`** — move to `archived/<class>/`, leave the Status line, add `Archived: YYYY-MM-DD — <reason>` directly below it.

Archive admission requires all three conditions: the decision is fully superseded or the mechanism is gone; it no longer guides future work; it retains historical value. Failing the third, **delete the record** — Git keeps the bytes.

A superseded record stays in its live bucket with both-way `Supersedes:` / `Superseded-by:` header lines. An archived record is no longer authority; active prose may link into it only to cite history.

## Deliberately absent

- `INDEX.md` — the path already carries bucket, class and date.
- Bilingual siblings — records are English-only, like every tracked page under `docs/` and `.agents/`.

## What the gate checks

`mstar lint .agents/adr --type adr` checks bucket/class membership, filename shape, `Status` agreeing with the directory, the `Archived:` line on archived records, balanced code fences, and the required and banned sections per bucket.

Two rules no tree walk can check: `Problem` must carry real stakes, and `Alternatives considered` must list options genuinely weighed — not strawmen built after the fact. Those remain review duties.
