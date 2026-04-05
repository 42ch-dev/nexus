# Domain Models Implementation Plan (Phase 1)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement domain model logic for Nexus core entities (Key Block, Timeline, Memory, SourceAnchor, Consistency Rules), extending generated Rust types with business logic, validation, and state transitions.

**Architecture:** Domain logic crate `nexus-domain` implementing business rules on top of generated contract types. Uses generated structs from `nexus-contracts` as base, adding methods, state machines, and consistency enforcement.

**Tech Stack:** Rust 1.75+, serde, thiserror, chrono (timestamps), uuid (identifiers), tokio (async)

**Branch Strategy:** Feature branch `feature/v1.0-domain-models` from `main`

---

## Files to Create

**Create crate: `crates/nexus-domain/`**
- `Cargo.toml` - Domain crate manifest
- `src/lib.rs` - Crate root
- `src/key_block.rs` - Key Block domain logic
- `src/timeline.rs` - Timeline domain logic
- `src/memory.rs` - Memory/MemoryItem domain logic
- `src/source_anchor.rs` - SourceAnchor domain logic
- `src/consistency.rs` - Consistency rules enforcement
- `src/manuscript_phase.rs` - Manuscript phase state machine
- `src/errors.rs` - Domain error types

---

## Working Branch

**Branch:** `feature/v1.0-domain-models`
**Base:** `main` (after Phase 0 complete)
**Strategy:** Create feature branch after Phase 0 initialization complete

---

## Task 1: Initialize Domain Crate

**Files:**
- Create: `crates/nexus-domain/Cargo.toml`
- Create: `crates/nexus-domain/src/lib.rs`

- [ ] **Step 1: Create feature branch**

Run: `git checkout -b feature/v1.0-domain-models`

Expected: Feature branch created from main

- [ ] **Step 2: Create domain crate directory**

Run: `mkdir -p crates/nexus-domain/src`

Expected: Directory created

- [ ] **Step 3: Create domain crate Cargo.toml**

Create file: `crates/nexus-domain/Cargo.toml`

```toml
[package]
name = "nexus-domain"
version = "0.1.0"
edition = "2021"
authors = ["42ch"]
license = "MIT"
repository = "https://github.com/42ch/nexus"

[dependencies]
nexus-contracts = { path = "../nexus-contracts" }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
thiserror = "1.0"
chrono = { version = "0.4", features = ["serde"] }
uuid = { version = "1.0", features = ["v4", "serde"] }
tokio = { version = "1.35", features = ["full"] }

[dev-dependencies]
tokio-test = "0.4"
```

Expected: Cargo.toml created

- [ ] **Step 4: Update root Cargo.toml to include domain crate**

Read `Cargo.toml`, add `nexus-domain` to workspace members:

```toml
members = [
    "crates/nexus-contracts",
    "crates/nexus-domain",
    # Future:
    # "crates/nexus42",
    # "crates/nexus42d",
]
```

Expected: Workspace updated

- [ ] **Step 5: Create domain crate lib.rs**

Create file: `crates/nexus-domain/src/lib.rs`

```rust
//! Nexus Domain Logic
//!
//! Business logic for core domain entities, extending generated wire contracts.

pub mod key_block;
pub mod timeline;
pub mod memory;
pub mod source_anchor;
pub mod consistency;
pub mod manuscript_phase;
pub mod errors;

pub use key_block::*;
pub use timeline::*;
pub use memory::*;
pub use source_anchor::*;
pub use consistency::*;
pub use manuscript_phase::*;
pub use errors::*;
```

Expected: lib.rs created

- [ ] **Step 6: Verify domain crate compiles**

Run: `cargo check -p nexus-domain`

Expected: Domain crate compiles (empty modules will warn, but no errors)

- [ ] **Step 7: Commit domain crate initialization**

Run: `git add Cargo.toml crates/nexus-domain && git commit -m "feat(domain): initialize domain logic crate"`

Expected: Commit successful

---

## Task 2: Implement Key Block Domain Logic

**Files:**
- Create: `crates/nexus-domain/src/key_block.rs`

- [ ] **Step 1: Write failing test for Key Block creation**

Create file: `crates/nexus-domain/src/key_block.rs`

```rust
use nexus_contracts::generated::*;
use uuid::Uuid;
use chrono::Utc;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_key_block() {
        let kb = KeyBlock::new(
            "world_123",
            KBType::Event,
            "Test Event",
        );
        
        assert!(kb.kb_ref.starts_with("kb_"));
        assert_eq!(kb.kb_type, KBType::Event);
        assert_eq!(kb.title, "Test Event");
    }

    #[test]
    fn test_confirm_key_block() {
        let mut kb = KeyBlock::new("world_123", KBType::Event, "Test");
        kb.confirm("creator_456");
        
        assert!(kb.is_confirmed());
        assert!(kb.confirmed_at.is_some());
    }
}
```

Expected: Test file created (will fail - methods not implemented)

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p nexus-domain --lib key_block::tests::test_create_key_block`

Expected: FAIL - `KeyBlock::new` method not found

- [ ] **Step 3: Implement Key Block domain extension**

Read `crates/nexus-domain/src/key_block.rs`, add implementation after tests:

```rust
use nexus_contracts::generated::*;
use uuid::Uuid;
use chrono::Utc;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum KBType {
    Event,
    Character,
    Location,
    Object,
    Concept,
    Relationship,
}

impl From<String> for KBType {
    fn from(s: String) -> Self {
        match s.as_str() {
            "event" => KBType::Event,
            "character" => KBType::Character,
            "location" => KBType::Location,
            "object" => KBType::Object,
            "concept" => KBType::Concept,
            "relationship" => KBType::Relationship,
            _ => KBType::Event,
        }
    }
}

impl KeyBlock {
    /// Create new Key Block
    pub fn new(world_ref: &str, kb_type: KBType, title: &str) -> Self {
        let kb_id = format!("kb_{}", Uuid::new_v4());
        let now = Utc::now().to_rfc3339();
        
        Self {
            kb_ref: kb_id,
            world_ref: world_ref.to_string(),
            kb_type: kb_type.to_string(),
            sequence: 0,
            title: title.to_string(),
            content: None,
            confirming_creator_id: None,
            confirmed_at: None,
            can_confirm_canon: false,
            source_anchor_refs: None,
            created_at: now,
            metadata: None,
            schema_version: "1.0.0".to_string(),
        }
    }

    /// Confirm this Key Block
    pub fn confirm(&mut self, creator_ref: &str) {
        self.confirming_creator_id = Some(creator_ref.to_string());
        self.confirmed_at = Some(Utc::now().to_rfc3339());
    }

    /// Check if Key Block is confirmed
    pub fn is_confirmed(&self) -> bool {
        self.confirmed_at.is_some()
    }

    /// Add source anchor reference
    pub fn add_source_anchor(&mut self, anchor_ref: &str) {
        if self.source_anchor_refs.is_none() {
            self.source_anchor_refs = Some(vec![]);
        }
        self.source_anchor_refs.as_mut().unwrap().push(anchor_ref.to_string());
    }
}

impl ToString for KBType {
    fn to_string(&self) -> String {
        match self {
            KBType::Event => "event",
            KBType::Character => "character",
            KBType::Location => "location",
            KBType::Object => "object",
            KBType::Concept => "concept",
            KBType::Relationship => "relationship",
        }.to_string()
    }
}
```

Expected: Key Block domain logic implemented

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p nexus-domain --lib key_block::tests`

Expected: PASS - all tests pass

- [ ] **Step 5: Commit Key Block implementation**

Run: `git add crates/nexus-domain/src/key_block.rs && git commit -m "feat(domain): implement Key Block domain logic with confirmation"`

Expected: Commit successful

---

## Task 3: Implement Timeline Domain Logic

**Files:**
- Create: `crates/nexus-domain/src/timeline.rs`

- [ ] **Step 1: Write failing test for Timeline operations**

Create file: `crates/nexus-domain/src/timeline.rs`

```rust
use nexus_contracts::generated::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_append_kb_to_timeline() {
        let mut timeline = Timeline::new("world_123");
        timeline.append_kb("kb_001");
        
        assert_eq!(timeline.kb_refs.len(), 1);
        assert_eq!(timeline.current_sequence, 1);
    }

    #[test]
    fn test_timeline_sequence_monotonic() {
        let mut timeline = Timeline::new("world_123");
        timeline.append_kb("kb_001");
        timeline.append_kb("kb_002");
        
        assert!(timeline.current_sequence > 0);
        assert_eq!(timeline.kb_refs.len(), 2);
    }
}
```

Expected: Test created (will fail)

- [ ] **Step 2: Implement Timeline domain extension**

Add to `crates/nexus-domain/src/timeline.rs`:

```rust
use nexus_contracts::generated::*;
use chrono::Utc;

impl Timeline {
    /// Create new Timeline
    pub fn new(world_ref: &str) -> Self {
        let now = Utc::now().to_rfc3339();
        
        Self {
            world_ref: world_ref.to_string(),
            current_sequence: 0,
            timeline_type: "main".to_string(),
            branch_parent_timeline_id: None,
            kb_refs: Some(vec![]),
            created_at: now,
            updated_at: Some(now),
            schema_version: "1.0.0".to_string(),
        }
    }

    /// Append Key Block to timeline
    pub fn append_kb(&mut self, kb_ref: &str) {
        if self.kb_refs.is_none() {
            self.kb_refs = Some(vec![]);
        }
        self.kb_refs.as_mut().unwrap().push(kb_ref.to_string());
        self.current_sequence += 1;
        self.updated_at = Some(Utc::now().to_rfc3339());
    }

    /// Get KB count in timeline
    pub fn kb_count(&self) -> usize {
        self.kb_refs.as_ref().map(|refs| refs.len()).unwrap_or(0)
    }

    /// Check if KB is in timeline
    pub fn contains_kb(&self, kb_ref: &str) -> bool {
        self.kb_refs
            .as_ref()
            .map(|refs| refs.contains(&kb_ref.to_string()))
            .unwrap_or(false)
    }

    /// Fork timeline
    pub fn fork(&self) -> Self {
        let forked_timeline = Timeline::new(&self.world_ref);
        forked_timeline
    }
}
```

Expected: Timeline domain logic implemented

- [ ] **Step 3: Run tests**

Run: `cargo test -p nexus-domain --lib timeline::tests`

Expected: PASS - timeline tests pass

- [ ] **Step 4: Commit Timeline implementation**

Run: `git add crates/nexus-domain/src/timeline.rs && git commit -m "feat(domain): implement Timeline domain logic with KB append and fork"`

Expected: Commit successful

---

## Task 4: Implement Memory Domain Logic

**Files:**
- Create: `crates/nexus-domain/src/memory.rs`

- [ ] **Step 1: Implement Memory domain extension**

Create file: `crates/nexus-domain/src/memory.rs`

```rust
use nexus_contracts::generated::*;
use chrono::Utc;
use uuid::Uuid;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum MemoryType {
    Experience,
    Soul,
    Knowledge,
    ReferenceExcerpt,
}

impl Memory {
    /// Create new Memory item
    pub fn new(world_ref: &str, memory_type: MemoryType, title: &str) -> Self {
        let memory_id = format!("memory_{}", Uuid::new_v4());
        let now = Utc::now().to_rfc3339();
        
        Self {
            memory_id,
            world_ref: world_ref.to_string(),
            creator_ref: None,
            memory_type: memory_type.to_string(),
            title: title.to_string(),
            content: Some(serde_json::json!({"text": ""})),
            kb_ref: None,
            source_anchor_ref: None,
            weight: Some(0.5),
            created_at: now,
            updated_at: Some(now),
            metadata: None,
            schema_version: "1.0.0".to_string(),
        }
    }

    /// Set memory weight
    pub fn set_weight(&mut self, weight: f64) {
        self.weight = Some(weight.clamp(0.0, 1.0));
    }

    /// Link to Key Block
    pub fn link_to_kb(&mut self, kb_ref: &str) {
        self.kb_ref = Some(kb_ref.to_string());
    }

    /// Add text content
    pub fn add_text(&mut self, text: &str) {
        self.content = Some(serde_json::json!({"text": text}));
    }
}

impl ToString for MemoryType {
    fn to_string(&self) -> String {
        match self {
            MemoryType::Experience => "experience",
            MemoryType::Soul => "soul",
            MemoryType::Knowledge => "knowledge",
            MemoryType::ReferenceExcerpt => "reference_excerpt",
        }.to_string()
    }
}
```

Expected: Memory domain logic implemented

- [ ] **Step 2: Write and run tests**

Add tests to `crates/nexus-domain/src/memory.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_memory() {
        let memory = Memory::new("world_123", MemoryType::Experience, "Test Memory");
        
        assert!(memory.memory_id.starts_with("memory_"));
        assert_eq!(memory.memory_type, "experience");
    }

    #[test]
    fn test_memory_weight_clamping() {
        let mut memory = Memory::new("world_123", MemoryType::Experience, "Test");
        memory.set_weight(1.5);
        
        assert_eq!(memory.weight, Some(1.0));
    }
}
```

Run: `cargo test -p nexus-domain --lib memory::tests`

Expected: PASS

- [ ] **Step 3: Commit Memory implementation**

Run: `git add crates/nexus-domain/src/memory.rs && git commit -m "feat(domain): implement Memory domain logic with weight and KB linking"`

Expected: Commit successful

---

## Task 5: Implement SourceAnchor and Consistency Rules

**Files:**
- Create: `crates/nexus-domain/src/source_anchor.rs`
- Create: `crates/nexus-domain/src/consistency.rs`
- Create: `crates/nexus-domain/src/errors.rs`

- [ ] **Step 1: Implement SourceAnchor**

Create file: `crates/nexus-domain/src/source_anchor.rs`

```rust
use chrono::Utc;
use uuid::Uuid;

/// SourceAnchor - Reference to story概要 in platform
/// (Note: Story概要 stored in platform, not in CLI schemas directly)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SourceAnchor {
    pub anchor_ref: String,
    pub story_ref: String,
    pub excerpt: String,
    pub created_at: String,
}

impl SourceAnchor {
    pub fn new(story_ref: &str, excerpt: &str) -> Self {
        let anchor_id = format!("anchor_{}", Uuid::new_v4());
        let now = Utc::now().to_rfc3339();
        
        Self {
            anchor_ref: anchor_id,
            story_ref: story_ref.to_string(),
            excerpt: excerpt.to_string(),
            created_at: now,
        }
    }

    /// Validate excerpt length (1024 chars max)
    pub fn validate_excerpt(&self) -> bool {
        self.excerpt.len() <= 1024
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_source_anchor_creation() {
        let anchor = SourceAnchor::new("story_123", "Test excerpt");
        
        assert!(anchor.anchor_ref.starts_with("anchor_"));
        assert_eq!(anchor.story_ref, "story_123");
    }

    #[test]
    fn test_excerpt_validation() {
        let short_excerpt = "Short text";
        let anchor = SourceAnchor::new("story_123", short_excerpt);
        
        assert!(anchor.validate_excerpt());
    }
}
```

Expected: SourceAnchor implemented

- [ ] **Step 2: Implement Consistency Rules**

Create file: `crates/nexus-domain/src/consistency.rs`

```rust
use crate::errors::DomainError;

/// Consistency rules enforcement
pub struct ConsistencyValidator;

impl ConsistencyValidator {
    /// Validate Key Block sequence is monotonic
    pub fn validate_kb_sequence(sequence: u64, prev_sequence: u64) -> Result<(), DomainError> {
        if sequence <= prev_sequence {
            return Err(DomainError::ConsistencyViolation(
                "Key Block sequence must be monotonic".to_string()
            ));
        }
        Ok(())
    }

    /// Validate provisional KB TTL (30 days max)
    pub fn validate_provisional_ttl(created_at: &str, now: &str) -> Result<(), DomainError> {
        use chrono::{DateTime, Utc, Duration};
        
        let created = DateTime::parse_from_rfc3339(created_at)
            .map_err(|_| DomainError::InvalidTimestamp)?
            .with_timezone(&Utc);
        
        let current = DateTime::parse_from_rfc3339(now)
            .map_err(|_| DomainError::InvalidTimestamp)?
            .with_timezone(&Utc);
        
        let ttl = Duration::days(30);
        if current - created > ttl {
            return Err(DomainError::ProvisionalTtlExpired);
        }
        
        Ok(())
    }

    /// Validate manuscript phase transition
    pub fn validate_phase_transition(
        from_phase: &str,
        to_phase: &str,
    ) -> Result<(), DomainError> {
        let valid_transitions = [
            ("brainstorm", "write"),
            ("write", "review"),
            ("review", "provisional"),
            ("provisional", "canon"),
        ];
        
        let transition = (from_phase, to_phase);
        if !valid_transitions.contains(&transition) {
            return Err(DomainError::InvalidPhaseTransition(
                format!("{} -> {}", from_phase, to_phase)
            ));
        }
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kb_sequence_validation() {
        assert!(ConsistencyValidator::validate_kb_sequence(2, 1).is_ok());
        assert!(ConsistencyValidator::validate_kb_sequence(1, 2).is_err());
    }
}
```

Expected: Consistency rules implemented

- [ ] **Step 3: Implement Domain Errors**

Create file: `crates/nexus-domain/src/errors.rs`

```rust
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DomainError {
    #[error("Consistency violation: {0}")]
    ConsistencyViolation(String),
    
    #[error("Provisional TTL expired")]
    ProvisionalTtlExpired,
    
    #[error("Invalid timestamp format")]
    InvalidTimestamp,
    
    #[error("Invalid phase transition: {0}")]
    InvalidPhaseTransition(String),
    
    #[error("Validation failed: {0}")]
    ValidationFailed(String),
    
    #[error("Key Block not found: {0}")]
    KeyBlockNotFound(String),
    
    #[error("Timeline error: {0}")]
    TimelineError(String),
    
    #[error("Memory error: {0}")]
    MemoryError(String),
    
    #[error("Source anchor error: {0}")]
    SourceAnchorError(String),
}
```

Expected: Domain errors implemented

- [ ] **Step 4: Run consistency tests**

Run: `cargo test -p nexus-domain --lib consistency::tests`

Expected: PASS

- [ ] **Step 5: Commit consistency and errors**

Run: `git add crates/nexus-domain/src/source_anchor.rs crates/nexus-domain/src/consistency.rs crates/nexus-domain/src/errors.rs && git commit -m "feat(domain): implement SourceAnchor, Consistency rules, and Domain errors"`

Expected: Commit successful

---

## Task 6: Implement Manuscript Phase State Machine

**Files:**
- Create: `crates/nexus-domain/src/manuscript_phase.rs`

- [ ] **Step 1: Implement manuscript phase logic**

Create file: `crates/nexus-domain/src/manuscript_phase.rs`

```rust
use crate::consistency::ConsistencyValidator;
use crate::errors::DomainError;

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum ManuscriptPhase {
    Brainstorm,
    Write,
    Review,
    Provisional,
    Canon,
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum ManuscriptState {
    Draft,
    Proposed,
    Confirmed,
    Published,
}

/// Manuscript phase state machine
pub struct ManuscriptStateMachine {
    phase: ManuscriptPhase,
    state: ManuscriptState,
}

impl ManuscriptStateMachine {
    pub fn new() -> Self {
        Self {
            phase: ManuscriptPhase::Brainstorm,
            state: ManuscriptState::Draft,
        }
    }

    /// Promote to next phase
    pub fn promote(&mut self) -> Result<(), DomainError> {
        let next_phase = match self.phase {
            ManuscriptPhase::Brainstorm => ManuscriptPhase::Write,
            ManuscriptPhase::Write => ManuscriptPhase::Review,
            ManuscriptPhase::Review => ManuscriptPhase::Provisional,
            ManuscriptPhase::Provisional => ManuscriptPhase::Canon,
            ManuscriptPhase::Canon => return Err(DomainError::InvalidPhaseTransition(
                "Canon is final phase".to_string()
            )),
        };
        
        ConsistencyValidator::validate_phase_transition(
            &self.phase.to_string(),
            &next_phase.to_string(),
        )?;
        
        self.phase = next_phase;
        self.state = ManuscriptState::Draft;
        
        Ok(())
    }

    /// Confirm in current phase
    pub fn confirm(&mut self) -> Result<(), DomainError> {
        if self.state != ManuscriptState::Proposed {
            return Err(DomainError::ValidationFailed(
                "Only proposed manuscripts can be confirmed".to_string()
            ));
        }
        
        self.state = ManuscriptState::Confirmed;
        Ok(())
    }

    /// Get current phase
    pub fn current_phase(&self) -> ManuscriptPhase {
        self.phase
    }

    /// Get current state
    pub fn current_state(&self) -> ManuscriptState {
        self.state
    }

    /// Check if canon
    pub fn is_canon(&self) -> bool {
        self.phase == ManuscriptPhase::Canon && self.state == ManuscriptState::Confirmed
    }
}

impl ToString for ManuscriptPhase {
    fn to_string(&self) -> String {
        match self {
            ManuscriptPhase::Brainstorm => "brainstorm",
            ManuscriptPhase::Write => "write",
            ManuscriptPhase::Review => "review",
            ManuscriptPhase::Provisional => "provisional",
            ManuscriptPhase::Canon => "canon",
        }.to_string()
    }
}

impl ToString for ManuscriptState {
    fn to_string(&self) -> String {
        match self {
            ManuscriptState::Draft => "draft",
            ManuscriptState::Proposed => "proposed",
            ManuscriptState::Confirmed => "confirmed",
            ManuscriptState::Published => "published",
        }.to_string()
    }
}
```

Expected: Manuscript phase state machine implemented

- [ ] **Step 2: Add tests**

Add tests to `crates/nexus-domain/src/manuscript_phase.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_phase_promotion() {
        let mut machine = ManuscriptStateMachine::new();
        
        assert_eq!(machine.current_phase(), ManuscriptPhase::Brainstorm);
        
        machine.promote().unwrap();
        assert_eq!(machine.current_phase(), ManuscriptPhase::Write);
        
        machine.promote().unwrap();
        assert_eq!(machine.current_phase(), ManuscriptPhase::Review);
    }

    #[test]
    fn test_invalid_promotion_from_canon() {
        let mut machine = ManuscriptStateMachine::new();
        
        // Promote to canon
        machine.promote().unwrap();
        machine.promote().unwrap();
        machine.promote().unwrap();
        machine.promote().unwrap();
        
        assert_eq!(machine.current_phase(), ManuscriptPhase::Canon);
        
        // Cannot promote further
        assert!(machine.promote().is_err());
    }
}
```

Run: `cargo test -p nexus-domain --lib manuscript_phase::tests`

Expected: PASS

- [ ] **Step 3: Commit manuscript phase**

Run: `git add crates/nexus-domain/src/manuscript_phase.rs && git commit -m "feat(domain): implement manuscript phase state machine"`

Expected: Commit successful

---

## Task 7: Final Verification and Documentation

**Files:**
- Modify: `crates/nexus-domain/src/lib.rs` (ensure all exports)
- Create: `crates/nexus-domain/README.md`

- [ ] **Step 1: Verify all domain tests pass**

Run: `cargo test -p nexus-domain --lib`

Expected: All domain tests pass

- [ ] **Step 2: Verify domain crate compiles**

Run: `cargo check -p nexus-domain`

Expected: No compilation errors

- [ ] **Step 3: Create domain crate README**

Create file: `crates/nexus-domain/README.md`

```markdown
# nexus-domain

Domain logic for Nexus core entities, extending generated wire contracts.

## Modules

- **key_block**: Key Block domain logic (creation, confirmation, source anchors)
- **timeline**: Timeline operations (append KB, fork, sequence management)
- **memory**: Memory/MemoryItem logic (weight, KB linking, content)
- **source_anchor**: SourceAnchor validation (excerpt length, story references)
- **consistency**: Consistency rules (sequence monotonicity, TTL, phase transitions)
- **manuscript_phase**: Manuscript phase state machine (BWR workflow)
- **errors**: Domain error types

## Usage

```rust
use nexus_domain::*;
use nexus_contracts::generated::*;

// Create and confirm Key Block
let mut kb = KeyBlock::new("world_123", KBType::Event, "Test Event");
kb.confirm("creator_456");
assert!(kb.is_confirmed());

// Append to Timeline
let mut timeline = Timeline::new("world_123");
timeline.append_kb("kb_001");

// Manuscript phase state machine
let mut machine = ManuscriptStateMachine::new();
machine.promote().unwrap(); // brainstorm -> write
```

## Tests

Run: `cargo test -p nexus-domain`
```

Expected: README created

- [ ] **Step 4: Commit documentation**

Run: `git add crates/nexus-domain/README.md && git commit -m "docs(domain): add domain crate README"`

Expected: Commit successful

---

## Verification

- [ ] **Final verification: All domain tests pass**

Run: `cargo test -p nexus-domain`

Expected: All tests pass, no compilation errors

- [ ] **Verify domain crate integration**

Run: `cargo build -p nexus-domain`

Expected: Domain crate builds successfully

- [ ] **Verify workspace integration**

Run: `cargo build --workspace`

Expected: Full workspace builds (contracts + domain)

---

## Completion

After all tasks complete:
- [ ] Update `.agents/plans/status.json` - mark domain-models as `completed`
- [ ] Push feature branch: `git push origin feature/v1.0-domain-models`
- [ ] Create PR or merge (user decision)

---

**Plan saved to:** `.agents/plans/2025-04-05-domain-models.md`