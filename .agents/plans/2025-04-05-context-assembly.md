# Context Assembly Basic Implementation Plan (Phase 1)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement Context Assembly basic version using Neo4j + Postgres/pgvector + HybridRAG baseline for narrative context retrieval.

**Architecture:** 
- **Neo4j**: Graph storage for Timeline, KB relationships, World network
- **Postgres/pgvector**: Vector storage for Memory embeddings, similarity search
- **HybridRAG**: Combined graph + vector retrieval for context assembly
- **Query Façade**: Unified API for context queries

**Tech Stack:** Rust 1.75+, tokio, neo4rs (Neo4j driver), sqlx (Postgres), async-openai (embeddings), pgvector

**Branch Strategy:** Feature branch `feature/v1.0-context-assembly` from `main`

---

## Working Branch

**Branch:** `feature/v1.0-context-assembly`
**Base:** `main` (after domain-models + sync-contract complete)

---

## Core Tasks Overview

### Task 1: Setup Infrastructure Containers
- Add `docker-compose.yml` for dev environment
- Postgres + pgvector container
- Neo4j container
- Redis container (for caching)
- Add `.env.example` with connection strings

### Task 2: Create Context Assembly Crate
- Create `crates/nexus-context/Cargo.toml`
- Implement Neo4j client wrapper
- Implement Postgres/pgvector client wrapper
- Add connection pooling

### Task 3: Implement Graph Storage
- Store Timeline in Neo4j
- Store KB relationships
- Implement graph traversal queries (timeline, KB lineage)
- Add Cypher query builders

### Task 4: Implement Vector Storage
- Store Memory embeddings in pgvector
- Implement similarity search
- Add embedding generation (OpenAI or local model)
- Implement HybridRAG query logic

### Task 5: Implement Query Façade
- Unified `ContextQuery` API
- Combine graph + vector results
- Add reranking (basic rules, LLM rerank future)
- Implement context assembly for CLI generation

### Task 6: Add CLI Integration
- CLI generates概要 locally
- Sends概要 to platform
- Platform validates and indexes

---

## Files to Create

**Context Assembly Crate (`crates/nexus-context/`):**
- `Cargo.toml`
- `src/lib.rs`
- `src/graph/` (Neo4j)
  - `mod.rs`
  - `client.rs`
  - `timeline.rs`
  - `kb_graph.rs`
- `src/vector/` (Postgres/pgvector)
  - `mod.rs`
  - `client.rs`
  - `embeddings.rs`
  - `similarity.rs`
- `src/hybrid/` (HybridRAG)
  - `mod.rs`
  - `query.rs`
  - `rerank.rs`
- `src/façade.rs` (Query façade)
- `src/errors.rs`

**Infrastructure:**
- `docker-compose.yml`
- `.env.example`

**Schemas:**
- `schemas/platform/context-assembly-v1.schema.json`

---

## Dev/Test Infrastructure Requirements

**Required Containers:**
```yaml
services:
  postgres:
    image: pgvector/pgvector:pg16
    ports: ["5432:5432"]
    environment:
      POSTGRES_DB: nexus
      POSTGRES_USER: nexus
      POSTGRES_PASSWORD: nexus

  neo4j:
    image: neo4j:5
    ports: ["7474:7474", "7687:7687"]
    environment:
      NEO4J_AUTH: neo4j/nexus

  redis:
    image: redis/redis-stack-server:latest
    ports: ["6379:6379"]
```

---

## Verification

- [ ] Docker Compose starts all services: `docker-compose up -d`
- [ ] Context crate compiles: `cargo build -p nexus-context`
- [ ] Neo4j connection works: basic node creation
- [ ] Postgres/pgvector connection works: vector similarity search
- [ ] HybridRAG query returns combined results
- [ ] Query façade provides unified API

---

**Plan saved to:** `.agents/plans/2025-04-05-context-assembly.md`