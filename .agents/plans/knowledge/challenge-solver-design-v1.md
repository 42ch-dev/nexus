# Challenge Solver Design v1

**Status**: Active — design SSOT for challenge-solver module
**Source plan**: V1.3 Creator Register CLI
**Compass**: [v1.3-delivery-compass-v1.md](v1.3-delivery-compass-v1.md)
**Platform spec reference**: v1.3-spec §4.3–4.5 (challenge generation rules)
**Created**: 2026-04-16

## 1) Purpose

This document specifies the design for the CLI-side challenge solver that parses obfuscated math challenges returned by the platform's `POST /creators/register` endpoint (v1.3-spec §4.3) and computes the answer for `POST /creators/verify` (v1.3-spec §4.5).

## 2) Platform contract summary

### Input (from platform response)

```json
{
  "verification": {
    "verification_code": "nxc_verify_abc123...",
    "challenge_text": "A bAs]KeT ^hAs tHiR*tY fI|vE ApPl-Es aNd ^sOmEoNe A*dDs ^TwEl/Ve Mo[Re, hOw MaN~y Ap-PlEs tO|tAl",
    "expires_at": "2026-04-16T00:05:00.000Z",
    "instructions": "Solve the math problem hidden in the challenge text."
  }
}
```

### Obfuscation layers (platform generates, CLI must reverse)

1. **Random case alternation**: `tHiRtY fIvE`
2. **Noise symbol insertion**: `]`, `^`, `*`, `|`, `-`, `~`, `/`, `[`
3. **Word-internal splitting**: `ApPl-Es`

### Math rules

- Template: "{scene} has {N1} {item} and someone {op} {N2} more, how many {item} total"
- Operations: `+`, `-`, `*`, `/`
- Operand range: 1–100
- Result: always non-negative integer
- Scene vocabulary: everyday English (basket, classroom, shelf, library, etc.)

### Output

```json
{
  "verification_code": "nxc_verify_abc123...",
  "answer": "47"
}
```

## 3) Architecture

### 3.1 Module layout

```
crates/nexus42/src/
├── commands/creator.rs          # CLI command entry (extended)
│   └── register subcommand
└── challenge/
    ├── mod.rs                    # Public API: solve_challenge(text) -> String
    ├── noise.rs                  # Noise removal layer
    ├── numbers.rs                # English number words → digits
    ├── parser.rs                 # Math problem extraction
    └── eval.rs                   # Arithmetic evaluation
```

**Alternative**: If the module is shared with daemon in the future, consider `crates/nexus-domain/src/challenge/`. For V1.3, CLI-only placement is sufficient.

### 3.2 Processing pipeline

```text
challenge_text (raw)
    │
    ▼
[1. Noise removal]  stripNoiseSymbols(text)
    │  Remove: ] ^ * | - ~ / [ (all 8 noise characters)
    │  Result: "A bAsKeT hAs tHiRtY fIvE ApPlEs aNd sOmEoNe AdDs tWeLvE MoRe..."
    │
    ▼
[2. Case normalization]  normalizeCase(text)
    │  Convert to lowercase
    │  Result: "a basket has thirty five apples and someone adds twelve more..."
    │
    ▼
[3. Word rejoining]  rejoinWords(text)
    │  Rejoin hyphenated/split words (hyphens already removed in step 1)
    │  Result: "a basket has thirty five apples and someone adds twelve more..."
    │
    ▼
[4. Number word conversion]  convertNumberWords(text)
    │  Map English number words to digit strings
    │  thirty five → "35", twelve → "12"
    │  Result: "a basket has 35 apples and someone adds 12 more..."
    │
    ▼
[5. Math extraction]  extractMathProblem(text)
    │  Extract two numbers and one operation
    │  Returns: { n1: 35, op: Add, n2: 12 }
    │
    ▼
[6. Evaluation]  evaluate(problem)
    │  Compute: 35 + 12 = 47
    │  Returns: "47"
```

### 3.3 Number word mapping

Support number words from 0 to 100 (operand range):

| Word | Digit | Word | Digit |
|------|-------|------|-------|
| zero | 0 | twenty | 20 |
| one | 1 | thirty | 30 |
| two | 2 | forty | 40 |
| ... | ... | fifty | 50 |
| ten | 10 | sixty | 60 |
| eleven | 11 | seventy | 70 |
| twelve | 12 | eighty | 80 |
| thirteen | 13 | ninety | 90 |
| ... | ... | hundred | 100 |
| nineteen | 19 | | |

**Compound numbers**: "thirty five" → 35, "twenty one" → 21. Parse as `<tens_word> <units_word>`.

### 3.4 Operation mapping

| Keywords | Operation |
|----------|-----------|
| adds, and, more, plus | `+` (Add) |
| subtracts, removes, takes, minus | `-` (Subtract) |
| multiplies, times | `*` (Multiply) |
| divides, splits, divided by | `/` (Divide) |

### 3.5 LLM fallback strategy

**Trigger**: Pure logic pipeline returns `None` (unrecognized pattern).

**Mechanism**: CLI invokes agent runtime LLM with `challenge-solver-skill.md` as system prompt.

**Skill file location**: `crates/nexus42/src/skills/challenge-solver-skill.md`

**Fallback prompt** (from v1.3-spec §9.3):

```markdown
# Creator Registration Challenge Solver

You are solving an anti-bot verification challenge during Creator registration.

## Input
You receive a `challenge_text` — a math problem disguised with noise.

## Task
1. Remove noise: strip all `]`, `^`, `*`, `|`, `-`, `~`, `/`, `[` symbols
2. Normalize casing: convert all text to lowercase
3. Rejoin hyphenated words: `ApPl-Es` → `apples`
4. Parse the math: extract two numbers and one operation (+, -, *, /)
5. Compute the answer
6. Return ONLY the numeric answer as a string

## Rules
- Numbers are always spelled out in English words
- Operations: "adds/and/more" = +, "subtracts/removes/takes" = -, "multiplies/times" = *, "divides/splits" = /
- Result is always a non-negative integer
- Return ONLY the number, no explanation
```

**Cost control**: LLM fallback is a single short inference call (~200 tokens). Not a concern for V1.3.

### 3.6 Error handling

| Scenario | Behavior |
|----------|----------|
| Challenge text empty/invalid | Return `Err(ChallengeError::InvalidInput)` |
| Unrecognized number word | Attempt LLM fallback; if fails, return error |
| Division by zero | Platform guarantees non-negative integer result; guard anyway |
| Non-integer result | Platform guarantees integer; if float, round to nearest |
| Challenge expired (5 min) | CLI should check `expires_at` before solving |

## 4) CLI command flow

```bash
nexus42 creator register --name "My Agent" [--source cli|web_agent]
```

### Internal flow

```text
1. CLI parses args (name, source)
2. CLI calls platform: POST {PLATFORM_URL}/api/v1/creators/register
   Body: { display_name: name, registration_source: source }
3. Parse response:
   - Save creator_id
   - Save creator_api_key (pending)
   - Extract verification_code, challenge_text, expires_at
4. Check expires_at > now (with 10s buffer)
5. Solve challenge: solve_challenge(challenge_text)
   - Pure logic pipeline (steps 1-6)
   - On failure: LLM fallback
6. Submit answer: POST {PLATFORM_URL}/api/v1/creators/verify
   Body: { verification_code, answer }
7. Handle response:
   - Success: store creator_api_key to local credentials
   - Wrong answer: display error + remaining attempts (auto-retry once max)
   - Expired: inform user to re-register
   - Locked: inform user account is permanently locked
```

### 4.1 Credential storage

**Preferred**: Extend daemon's `auth_tokens` table with a `creator_api_key` column (or new `creator_credentials` table).

**Alternative**: Store in CLI's `~/.nexus42/auth.json` (existing `CreatorAuthState`).

**Decision**: Use daemon storage if daemon is running; fall back to CLI file store. Check daemon health before deciding.

## 5) Platform API client extension

### Option A: Extend SyncClient

Add `register_creator()` and `verify_creator()` methods to existing `nexus-sync/src/sync_client.rs`.

**Pros**: Single HTTP client, shared config.
**Cons**: SyncClient is sync-specific; mixing creator registration concerns.

### Option B: New PlatformClient module

Create `crates/nexus-sync/src/platform_client.rs` (or a new small crate) for non-sync platform interactions (creator registration, future entitlements, etc.).

**Pros**: Clean separation of concerns.
**Cons**: New module to maintain.

**Recommendation**: Option B (new `PlatformClient`) — the registration/verification flow is semantically distinct from sync operations. The `PlatformClient` can share the same `reqwest` client and base URL config from SyncClient.

## 6) Test strategy

| Test type | Scope | Mock level |
|-----------|-------|------------|
| Unit: noise removal | `noise.rs` | None (pure function) |
| Unit: number conversion | `numbers.rs` | None (pure function) |
| Unit: math extraction | `parser.rs` | None (pure function) |
| Unit: evaluation | `eval.rs` | None (pure function) |
| Integration: full solver | `mod.rs` | None (all pure) |
| Integration: CLI command | `commands/creator.rs` | Mock HTTP server (wire responses) |
| E2E: register→verify | Full CLI | Mock platform API |

### 6.1 Test cases for solver

| Input | Expected | Notes |
|-------|----------|-------|
| Standard addition | "47" | Spec example |
| Standard subtraction | Result | Non-negative check |
| Standard multiplication | Result | |
| Standard division | Result | Integer division |
| Extra noise symbols | Correct | Robustness |
| Unusual number words | Correct via LLM fallback | Edge case |
| Empty input | Error | Validation |

## 7) Open decisions

| # | Decision | Options | Status |
|---|----------|---------|--------|
| D1 | Module placement (CLI vs domain) | `crates/nexus42/src/challenge/` (V1.3) vs `crates/nexus-domain/src/challenge/` | Tentative: CLI-only for V1.3 |
| D2 | PlatformClient crate vs module | New file in nexus-sync vs new crate | Tentative: module in nexus-sync |
| D3 | Credential storage location | Daemon DB vs CLI file | Tentative: daemon-first with CLI fallback |
| D4 | Auto-retry on wrong answer | 0 or 1 auto-retry | Open (spec allows multiple attempts) |
