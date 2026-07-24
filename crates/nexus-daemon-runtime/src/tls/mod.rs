//! TLS certificate management for the daemon runtime.
//!
//! Auto-generates and persists an Ed25519 self-signed certificate under
//! `~/.nexus42/tls/` for non-loopback daemon binds. Loopback binds remain
//! plain HTTP and do not load or generate a certificate.

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::time::SystemTime;

use anyhow::Context;
use nexus_contracts::CertFingerprintResponse;
use rcgen::{CertificateParams, DistinguishedName, DnType, KeyPair, PKCS_ED25519};
use sha2::{Digest, Sha256};
use tokio::fs;
use tracing::info;

/// Load an existing TLS certificate/key pair from `~/.nexus42/tls/`, or
/// generate a new Ed25519 self-signed pair if files are missing or cannot
/// be parsed.
///
/// `bind_host` is threaded into generated certificates as a Subject Alternative
/// Name (IP address if it parses as an IP, otherwise DNS name), so remote
/// clients connecting to the actual bind host can validate the TLS handshake.
/// Loopback hosts produce loopback-only SANs.
///
/// Returns the `axum-server` TLS config plus the public fingerprint used for
/// TOFU pinning. The fingerprint is computed as SHA-256 of the DER-encoded
/// certificate, formatted as `SHA256:<colon-hex>`.
///
/// # Errors
///
/// Returns an error if the TLS directory or files cannot be created, written,
/// read, or parsed into a usable `RustlsConfig`.
pub async fn load_or_generate_tls_config(
    home: &Path,
    bind_host: &str,
) -> anyhow::Result<(
    axum_server::tls_rustls::RustlsConfig,
    CertFingerprintResponse,
)> {
    let cert_path = nexus_home_layout::tls_cert_path(home);
    let key_path = nexus_home_layout::tls_key_path(home);

    if let Some((config, existing)) = try_load_existing(&cert_path, &key_path, bind_host).await {
        info!(
            cert_path = %cert_path.display(),
            fingerprint = %existing.fingerprint,
            bind_host = %bind_host,
            "Loaded existing TLS certificate"
        );
        return Ok((config, existing));
    }

    generate_and_persist(home, &cert_path, &key_path, bind_host).await
}

/// Attempt to reuse an existing certificate/key pair.
///
/// Returns `None` if files are missing, cannot be parsed, or do not cover the
/// current `bind_host` as a SAN, signalling that a new pair should be generated.
async fn try_load_existing(
    cert_path: &Path,
    key_path: &Path,
    bind_host: &str,
) -> Option<(
    axum_server::tls_rustls::RustlsConfig,
    CertFingerprintResponse,
)> {
    let cert_bytes = fs::read(cert_path).await.ok()?;
    let key_bytes = fs::read(key_path).await.ok()?;

    let certs: Vec<Vec<u8>> = rustls_pemfile::certs(&mut cert_bytes.as_slice())
        .collect::<Result<Vec<_>, _>>()
        .ok()?
        .into_iter()
        .map(|cert| cert.to_vec())
        .collect();
    if certs.is_empty() {
        return None;
    }

    // Verify the files form a usable TLS config before claiming success.
    let config = axum_server::tls_rustls::RustlsConfig::from_pem(cert_bytes, key_bytes)
        .await
        .ok()?;

    if !cert_covers_bind_host(&certs[0], bind_host) {
        info!(
            %bind_host,
            cert_path = %cert_path.display(),
            "Existing TLS certificate SAN does not cover current bind host; regenerating certificate"
        );
        return None;
    }

    let fingerprint = fingerprint_from_der(&certs[0]);
    let created_at = cert_metadata_created_at(cert_path).await;

    Some((
        config,
        CertFingerprintResponse {
            fingerprint,
            algorithm: "sha256".parse().expect("valid algorithm constant"),
            created_at: created_at.and_then(|s| {
                chrono::DateTime::parse_from_rfc3339(&s)
                    .ok()
                    .map(|dt| dt.with_timezone(&chrono::Utc))
            }),
        },
    ))
}

/// Generate a fresh Ed25519 self-signed certificate, persist it, and return
/// the TLS config plus fingerprint.
async fn generate_and_persist(
    home: &Path,
    cert_path: &Path,
    key_path: &Path,
    bind_host: &str,
) -> anyhow::Result<(
    axum_server::tls_rustls::RustlsConfig,
    CertFingerprintResponse,
)> {
    let tls_dir = nexus_home_layout::tls_dir(home);

    if !tls_dir.exists() {
        fs::create_dir_all(&tls_dir)
            .await
            .context("create tls directory")?;
    }
    #[cfg(unix)]
    {
        fs::set_permissions(&tls_dir, std::fs::Permissions::from_mode(0o700))
            .await
            .context("set tls directory permissions")?;
    }

    let key_pair = KeyPair::generate_for(&PKCS_ED25519).context("generate Ed25519 key pair")?;

    let mut params = CertificateParams::default();
    let mut dn = DistinguishedName::new();
    dn.push(DnType::CommonName, "nexus42-daemon");
    params.distinguished_name = dn;
    params.subject_alt_names = build_subject_alt_names(bind_host);

    let cert = params
        .self_signed(&key_pair)
        .context("self-sign TLS certificate")?;

    let cert_pem = cert.pem();
    let key_pem = key_pair.serialize_pem();

    write_private_file(cert_path, cert_pem.as_bytes())
        .await
        .context("write cert.pem")?;
    write_private_file(key_path, key_pem.as_bytes())
        .await
        .context("write key.pem")?;

    let fingerprint = fingerprint_from_der(cert.der());
    let created_at = chrono::Utc::now();
    let response = CertFingerprintResponse {
        fingerprint: fingerprint.clone(),
        algorithm: "sha256".parse().expect("valid algorithm constant"),
        created_at: Some(created_at),
    };

    info!(
        fingerprint = %fingerprint,
        cert_path = %cert_path.display(),
        "Generated new TLS certificate"
    );

    let config = axum_server::tls_rustls::RustlsConfig::from_pem(
        cert_pem.as_bytes().to_vec(),
        key_pem.as_bytes().to_vec(),
    )
    .await
    .context("RustlsConfig::from_pem with generated certificate")?;

    Ok((config, response))
}

/// Build the SAN list for the daemon certificate.
///
/// Always includes loopback addresses so local clients and loopback-first
/// generation work. For non-loopback concrete bind hosts, also includes the
/// bind host as an IP or DNS SAN. Wildcard bind addresses (`0.0.0.0`, `::`)
/// are skipped because they are not valid server names for TLS hostname
/// validation; clients in that case must connect via a concrete address whose
/// SAN is present (loopback for local tests) or use fingerprint pinning with
/// a custom verifier.
fn build_subject_alt_names(bind_host: &str) -> Vec<rcgen::SanType> {
    let mut sans = vec![
        rcgen::SanType::IpAddress(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST)),
        rcgen::SanType::IpAddress(std::net::IpAddr::V6(std::net::Ipv6Addr::LOCALHOST)),
        rcgen::SanType::DnsName(
            rcgen::Ia5String::try_from("localhost".to_string()).expect("localhost is valid Ia5"),
        ),
    ];

    let trimmed = bind_host.trim();
    if trimmed.is_empty() || trimmed == "0.0.0.0" || trimmed == "::" {
        return sans;
    }

    if let Ok(ip) = trimmed.parse::<std::net::IpAddr>() {
        if !ip.is_loopback() {
            sans.push(rcgen::SanType::IpAddress(ip));
        }
    } else {
        match rcgen::Ia5String::try_from(trimmed.to_string()) {
            Ok(name) => sans.push(rcgen::SanType::DnsName(name)),
            Err(e) => tracing::warn!(
                %bind_host,
                error = %e,
                "bind host is not a valid IA5 DNS name; omitting from cert SAN"
            ),
        }
    }

    sans
}

/// Check whether a persisted certificate's SAN list covers `bind_host`.
///
/// Loopback hosts and wildcard bind addresses are always considered covered
/// (loopback SANs are always present; wildcards are intentionally not valid
/// server names and are skipped at generation time).
fn cert_covers_bind_host(cert_der: &[u8], bind_host: &str) -> bool {
    use x509_parser::extensions::GeneralName;

    let trimmed = bind_host.trim();
    if trimmed.is_empty()
        || trimmed.eq_ignore_ascii_case("localhost")
        || trimmed == "0.0.0.0"
        || trimmed == "::"
    {
        return true;
    }
    if let Ok(ip) = trimmed.parse::<std::net::IpAddr>() {
        if ip.is_loopback() {
            return true;
        }
    }

    let Ok((_, cert)) = x509_parser::parse_x509_certificate(cert_der) else {
        return false;
    };

    let sans = match cert.subject_alternative_name() {
        Ok(Some(ext)) => &ext.value.general_names,
        _ => return false,
    };

    let Ok(ip) = trimmed.parse::<std::net::IpAddr>() else {
        return sans.iter().any(|gn| match gn {
            GeneralName::DNSName(name) => name.eq_ignore_ascii_case(trimmed),
            _ => false,
        });
    };

    sans.iter().any(|gn| match gn {
        GeneralName::IPAddress(der_bytes) => {
            let der_bytes: &[u8] = der_bytes;
            let parsed: Option<std::net::IpAddr> = match der_bytes.len() {
                4 => {
                    let arr: [u8; 4] = match der_bytes.try_into() {
                        Ok(a) => a,
                        Err(_) => return false,
                    };
                    Some(std::net::IpAddr::V4(std::net::Ipv4Addr::from(arr)))
                }
                16 => {
                    let arr: [u8; 16] = match der_bytes.try_into() {
                        Ok(a) => a,
                        Err(_) => return false,
                    };
                    Some(std::net::IpAddr::V6(std::net::Ipv6Addr::from(arr)))
                }
                _ => None,
            };
            parsed == Some(ip)
        }
        _ => false,
    })
}

/// Write `contents` to `path` with owner-only permissions (`0o600`).
async fn write_private_file(path: &Path, contents: &[u8]) -> anyhow::Result<()> {
    fs::write(path, contents).await?;
    #[cfg(unix)]
    {
        fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).await?;
    }
    Ok(())
}

/// Compute the SHA-256 fingerprint of a DER-encoded certificate.
fn fingerprint_from_der(der: &[u8]) -> String {
    let digest = Sha256::digest(der);
    let hex = digest
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(":");
    format!("SHA256:{hex}")
}

/// Best-effort created-at timestamp derived from the certificate file mtime.
async fn cert_metadata_created_at(cert_path: &Path) -> Option<String> {
    let meta = fs::metadata(cert_path).await.ok()?;
    let modified = meta.modified().ok()?;
    Some(system_time_to_rfc3339(modified))
}

/// Convert a `SystemTime` to an RFC 3339 string.
fn system_time_to_rfc3339(t: SystemTime) -> String {
    let duration = t.duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default();
    let datetime =
        chrono::DateTime::UNIX_EPOCH + chrono::Duration::seconds(duration.as_secs().cast_signed());
    datetime.to_rfc3339()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ensure_crypto_provider() {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    }

    #[tokio::test]
    async fn generate_and_reuse_tls_cert() {
        ensure_crypto_provider();
        let tmp = tempfile::TempDir::new().expect("temp dir");
        let home = tmp.path();

        let (config1, fp1) = load_or_generate_tls_config(home, "127.0.0.1")
            .await
            .expect("first generation");
        assert!(fp1.fingerprint.starts_with("SHA256:"));
        assert_eq!(fp1.algorithm.to_string(), "sha256");
        assert!(fp1.created_at.is_some());

        // The file-backed config must be usable.
        let _ = config1;

        let (_config2, fp2) = load_or_generate_tls_config(home, "127.0.0.1")
            .await
            .expect("second load");
        assert_eq!(fp1.fingerprint, fp2.fingerprint);
    }

    #[tokio::test]
    async fn rebind_to_different_host_regenerates_cert() {
        ensure_crypto_provider();
        let tmp = tempfile::TempDir::new().expect("temp dir");
        let home = tmp.path();

        let (_config_a, fp_a) = load_or_generate_tls_config(home, "192.168.1.10")
            .await
            .expect("generate host A");
        assert!(fp_a.fingerprint.starts_with("SHA256:"));

        // Loading with a different bind host must regenerate the certificate
        // so that its SAN covers the current host.
        let (_config_b, fp_b) = load_or_generate_tls_config(home, "192.168.1.11")
            .await
            .expect("regenerate host B");
        assert!(fp_b.fingerprint.starts_with("SHA256:"));
        assert_ne!(fp_a.fingerprint, fp_b.fingerprint);

        let cert_path = nexus_home_layout::tls_cert_path(home);
        let cert_bytes = fs::read(&cert_path).await.expect("read cert");
        let certs: Vec<Vec<u8>> = rustls_pemfile::certs(&mut cert_bytes.as_slice())
            .collect::<Result<Vec<_>, _>>()
            .expect("parse cert")
            .into_iter()
            .map(|cert| cert.to_vec())
            .collect();
        assert!(cert_covers_bind_host(&certs[0], "192.168.1.11"));
        assert!(!cert_covers_bind_host(&certs[0], "192.168.1.10"));
    }

    #[tokio::test]
    async fn ipv6_non_loopback_bind_host_is_covered_by_san() {
        ensure_crypto_provider();
        let tmp = tempfile::TempDir::new().expect("temp dir");
        let home = tmp.path();

        let (_config, fp) = load_or_generate_tls_config(home, "fd00::1")
            .await
            .expect("generate IPv6 bind cert");
        assert!(fp.fingerprint.starts_with("SHA256:"));

        let cert_path = nexus_home_layout::tls_cert_path(home);
        let cert_bytes = fs::read(&cert_path).await.expect("read cert");
        let certs: Vec<Vec<u8>> = rustls_pemfile::certs(&mut cert_bytes.as_slice())
            .collect::<Result<Vec<_>, _>>()
            .expect("parse cert")
            .into_iter()
            .map(|cert| cert.to_vec())
            .collect();

        assert!(cert_covers_bind_host(&certs[0], "fd00::1"));
        assert!(!cert_covers_bind_host(&certs[0], "fd00::2"));
    }

    #[test]
    fn build_sans_includes_non_loopback_bind_host_ip() {
        let sans = build_subject_alt_names("192.168.1.42");
        let sans_str: Vec<String> = sans.iter().map(san_to_string).collect();
        assert!(sans_str.contains(&"192.168.1.42".to_string()));
        assert!(sans_str.contains(&"127.0.0.1".to_string()));
        assert!(sans_str.contains(&"::1".to_string()));
        assert!(sans_str.contains(&"localhost".to_string()));
    }

    #[test]
    fn build_sans_includes_non_loopback_bind_host_dns() {
        let sans = build_subject_alt_names("nexus.local");
        let sans_str: Vec<String> = sans.iter().map(san_to_string).collect();
        assert!(sans_str.contains(&"nexus.local".to_string()));
    }

    #[test]
    fn build_sans_skips_wildcard_bind_hosts() {
        let sans = build_subject_alt_names("0.0.0.0");
        let sans_str: Vec<String> = sans.iter().map(san_to_string).collect();
        assert!(!sans_str.contains(&"0.0.0.0".to_string()));
        assert!(sans_str.contains(&"127.0.0.1".to_string()));
    }

    fn san_to_string(san: &rcgen::SanType) -> String {
        match san {
            rcgen::SanType::IpAddress(ip) => ip.to_string(),
            rcgen::SanType::DnsName(name) => name.as_str().to_string(),
            other => panic!("unexpected SAN variant: {other:?}"),
        }
    }

    #[test]
    fn fingerprint_format_is_colon_hex_with_prefix() {
        let der = vec![0xab; 32];
        let fp = fingerprint_from_der(&der);
        assert!(fp.starts_with("SHA256:"));
        let hex = &fp[7..];
        assert_eq!(hex.matches(':').count(), 31);
    }

    #[tokio::test]
    async fn corrupt_existing_files_are_regenerated() {
        ensure_crypto_provider();
        let tmp = tempfile::TempDir::new().expect("temp dir");
        let home = tmp.path();
        let cert_path = nexus_home_layout::tls_cert_path(home);
        let key_path = nexus_home_layout::tls_key_path(home);

        fs::create_dir_all(nexus_home_layout::tls_dir(home))
            .await
            .unwrap();
        fs::write(&cert_path, "not a cert").await.unwrap();
        fs::write(&key_path, "not a key").await.unwrap();

        let (_config, fp) = load_or_generate_tls_config(home, "127.0.0.1")
            .await
            .expect("regenerate after corrupt files");
        assert!(fp.fingerprint.starts_with("SHA256:"));
        assert!(cert_path.exists());
        assert!(key_path.exists());
    }

    #[tokio::test]
    async fn permissions_are_owner_only() {
        ensure_crypto_provider();
        let tmp = tempfile::TempDir::new().expect("temp dir");
        let home = tmp.path();

        let _ = load_or_generate_tls_config(home, "127.0.0.1")
            .await
            .unwrap();

        let dir_mode = fs::metadata(nexus_home_layout::tls_dir(home))
            .await
            .unwrap()
            .permissions()
            .mode();
        let key_mode = fs::metadata(nexus_home_layout::tls_key_path(home))
            .await
            .unwrap()
            .permissions()
            .mode();
        let cert_mode = fs::metadata(nexus_home_layout::tls_cert_path(home))
            .await
            .unwrap()
            .permissions()
            .mode();

        // Mask off the high bits (setuid/setgid/sticky) to keep just rwx.
        assert_eq!(dir_mode & 0o777, 0o700);
        assert_eq!(key_mode & 0o777, 0o600);
        assert_eq!(cert_mode & 0o777, 0o600);
    }

    #[test]
    fn system_time_to_rfc3339_produces_valid_string() {
        let t = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
        let s = system_time_to_rfc3339(t);
        assert!(s.contains('T'));
        assert!(s.contains('+') || s.contains('Z'));
    }
}
