//! Fail-closed fallback for a platform with neither the Unix nor the Windows
//! access-policy contract.
//!
//! Such a platform has no supported way to keep the old document's policy, so
//! capturing it fails and the publisher refuses to replace an existing document
//! instead of silently dropping the policy.

use std::fs::File;
use std::io;
use std::path::Path;

/// No supported policy exists on this platform.
pub struct PortablePolicy;

pub fn capture(_target: &Path) -> io::Result<PortablePolicy> {
    Err(unsupported())
}

pub fn apply(_policy: &PortablePolicy, _file: &File, _path: &Path) -> io::Result<()> {
    Err(unsupported())
}

pub fn access_policy_blob(_target: &Path) -> io::Result<Vec<u8>> {
    Err(unsupported())
}

fn unsupported() -> io::Error {
    io::Error::new(
        io::ErrorKind::Unsupported,
        "access-policy-preserving config publication is not supported on this platform",
    )
}
