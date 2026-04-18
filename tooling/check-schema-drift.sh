#!/usr/bin/env bash
# Schema / ownership consistency checks for nexus-local-db vs CLI/daemon.
# Mirrors `.github/workflows/ci.yml` job `schema-consistency-check`.
# Usage (from repository root): bash tooling/check-schema-drift.sh

set -eu

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

echo "==> Checking CLI and daemon both depend on nexus-local-db..."
if ! grep -q 'nexus-local-db' crates/nexus42/Cargo.toml; then
  echo "❌ CLI does not depend on nexus-local-db"
  exit 1
fi
if ! grep -q 'nexus-local-db' crates/nexus42d/Cargo.toml; then
  echo "❌ Daemon does not depend on nexus-local-db"
  exit 1
fi
echo "✅ Both CLI and daemon depend on nexus-local-db"

echo "==> Checking DB_SCHEMA_VERSION location (single ownership)..."
if ! grep -q 'pub const DB_SCHEMA_VERSION: u32' crates/nexus-local-db/src/version.rs; then
  echo "❌ DB_SCHEMA_VERSION not found in nexus-local-db/src/version.rs"
  exit 1
fi
CLI_HAS_CONST=$(grep -E 'pub const DB_SCHEMA_VERSION: u32 = [0-9]+' crates/nexus42/src/db/mod.rs 2>/dev/null | wc -l | tr -d ' ')
DAEMON_HAS_CONST=$(grep -E 'pub const DB_SCHEMA_VERSION: u32 = [0-9]+' crates/nexus42d/src/db/schema.rs 2>/dev/null | wc -l | tr -d ' ')
if [ "$CLI_HAS_CONST" != "0" ]; then
  echo "❌ CLI has duplicated DB_SCHEMA_VERSION constant (should import from nexus-local-db)"
  exit 1
fi
if [ "$DAEMON_HAS_CONST" != "0" ]; then
  echo "❌ Daemon has duplicated DB_SCHEMA_VERSION constant (should import from nexus-local-db)"
  exit 1
fi
echo "✅ DB_SCHEMA_VERSION is defined only in nexus-local-db"

echo "==> Checking SCHEMA_VERSION source from generated contracts..."
if ! grep -q 'pub use nexus_contracts::generated::LATEST_SCHEMA_VERSION as SCHEMA_VERSION' crates/nexus-local-db/src/version.rs; then
  echo "❌ SCHEMA_VERSION is not sourced from nexus-contracts::generated::LATEST_SCHEMA_VERSION"
  exit 1
fi
if ! grep -q 'pub const LATEST_SCHEMA_VERSION: u32' crates/nexus-contracts/src/generated/mod.rs; then
  echo "❌ LATEST_SCHEMA_VERSION not found in nexus-contracts generated code"
  exit 1
fi
echo "✅ SCHEMA_VERSION is sourced from nexus-contracts generated constants"

echo "==> Checking Rust vs TypeScript LATEST_SCHEMA_VERSION numeric parity..."
RSV=$(grep -E '^pub const LATEST_SCHEMA_VERSION: u32 = [0-9]+;' crates/nexus-contracts/src/generated/mod.rs | sed -E 's/.*= ([0-9]+);/\1/')
TSV=$(grep -E '^export const LATEST_SCHEMA_VERSION = [0-9]+;' packages/nexus-contracts/src/generated/index.ts | sed -E 's/.*= ([0-9]+);/\1/')
if [ -z "$RSV" ] || [ -z "$TSV" ]; then
  echo "❌ Could not parse LATEST_SCHEMA_VERSION from generated Rust or TS"
  exit 1
fi
if [ "$RSV" != "$TSV" ]; then
  echo "❌ LATEST_SCHEMA_VERSION mismatch: Rust=$RSV TypeScript=$TSV"
  exit 1
fi
echo "✅ LATEST_SCHEMA_VERSION matches between Rust (u32) and TypeScript (number)"

echo "==> Checking no duplicated shared table DDL..."
for table in workspace_meta creators reference_sources; do
  CLI_DDL=$(grep -r "CREATE TABLE IF NOT EXISTS $table" crates/nexus42/src/db/ 2>/dev/null | grep -v test | wc -l | tr -d ' ')
  DAEMON_DDL=$(grep -r "CREATE TABLE IF NOT EXISTS $table" crates/nexus42d/src/db/ 2>/dev/null | grep -v test | wc -l | tr -d ' ')
  LOCALDB_DDL=$(grep -r "CREATE TABLE IF NOT EXISTS $table" crates/nexus-local-db/migrations/ 2>/dev/null | wc -l | tr -d ' ')
  if [ "$CLI_DDL" != "0" ]; then
    echo "❌ CLI has duplicated DDL for $table table (should use nexus-local-db)"
    exit 1
  fi
  if [ "$DAEMON_DDL" != "0" ]; then
    echo "❌ Daemon has duplicated DDL for $table table (should use nexus-local-db)"
    exit 1
  fi
  if [ "$LOCALDB_DDL" == "0" ]; then
    echo "❌ $table table DDL not found in nexus-local-db"
    exit 1
  fi
done
echo "✅ No duplicated DDL - shared tables defined only in nexus-local-db"

echo "==> Checking no deprecated WIRE_SCHEMA_VERSION..."
WIRE_IN_CLI=$(grep -r "WIRE_SCHEMA_VERSION" crates/nexus42/src/ 2>/dev/null | wc -l | tr -d ' ')
WIRE_IN_DAEMON=$(grep -r "WIRE_SCHEMA_VERSION" crates/nexus42d/src/ 2>/dev/null | wc -l | tr -d ' ')
WIRE_IN_LOCALDB=$(grep -r "WIRE_SCHEMA_VERSION" crates/nexus-local-db/src/ 2>/dev/null | wc -l | tr -d ' ')
if [ "$WIRE_IN_CLI" != "0" ]; then
  echo "❌ Deprecated WIRE_SCHEMA_VERSION found in CLI (should use schema_version)"
  exit 1
fi
if [ "$WIRE_IN_DAEMON" != "0" ]; then
  echo "❌ Deprecated WIRE_SCHEMA_VERSION found in daemon (should use schema_version)"
  exit 1
fi
if [ "$WIRE_IN_LOCALDB" != "0" ]; then
  echo "❌ Deprecated WIRE_SCHEMA_VERSION found in nexus-local-db (should use schema_version)"
  exit 1
fi
echo "✅ No deprecated WIRE_SCHEMA_VERSION - using schema_version instead"

echo "==> Checking CLI/daemon use nexus-local-db API..."
if ! grep -q 'use nexus_local_db::' crates/nexus42/src/db/mod.rs; then
  echo "❌ CLI does not import from nexus_local_db"
  exit 1
fi
if ! grep -q 'use nexus_local_db::' crates/nexus42d/src/db/schema.rs; then
  echo "❌ Daemon does not import from nexus_local_db"
  exit 1
fi
echo "✅ Both CLI and daemon use nexus-local-db API"

echo "✅ All schema consistency checks passed."
