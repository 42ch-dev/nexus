use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

fn repo_root() -> PathBuf {
    PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("repo root")
        .to_path_buf()
}

fn collect_schema_files(dir: &Path, base: &Path, out: &mut Vec<(String, Vec<u8>)>) {
    for entry in fs::read_dir(dir).expect("read schemas dir") {
        let entry = entry.expect("schema entry");
        let path = entry.path();
        if path.is_dir() {
            collect_schema_files(&path, base, out);
            continue;
        }
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        if !path.file_name().and_then(|n| n.to_str()).unwrap_or("").ends_with(".schema.json") {
            continue;
        }
        let rel = path
            .strip_prefix(base)
            .expect("schema under base")
            .to_string_lossy()
            .replace('\\', "/");
        let bytes = fs::read(&path).expect("read schema");
        out.push((rel, bytes));
    }
}

fn contract_tree_sha256(schemas_dir: &Path) -> String {
    let mut entries: Vec<(String, Vec<u8>)> = Vec::new();
    collect_schema_files(schemas_dir, schemas_dir, &mut entries);
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    let mut hasher = Sha256::new();
    for (rel, bytes) in entries {
        hasher.update(rel.as_bytes());
        hasher.update(&bytes);
    }
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{:02x}", byte))
        .collect::<String>()
}

fn main() {
    napi_build::setup();
    let target = std::env::var("TARGET").unwrap_or_else(|_| "unknown-unknown-unknown".to_string());
    println!("cargo:rustc-env=NEXUS_BUILD_TARGET={target}");

    let repo = repo_root();
    let schemas_dir = repo.join("schemas");
    let hash = contract_tree_sha256(&schemas_dir);
    println!("cargo:rustc-env=NEXUS_CONTRACT_TREE_SHA256={hash}");

    let db_schema_version = nexus_local_db::DB_SCHEMA_VERSION;
    println!("cargo:rustc-env=NEXUS_DB_SCHEMA_MIN={db_schema_version}");
    println!("cargo:rustc-env=NEXUS_DB_SCHEMA_MAX={db_schema_version}");
}
