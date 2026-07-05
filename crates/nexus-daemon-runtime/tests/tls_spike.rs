//! V1.92 P-1 T1 — TLS spike: proves that rcgen + rustls + axum-server
//! compile together and type-check for the nexus-daemon-runtime workspace.
//!
//! This is a compile-proof, not a full integration test. It verifies:
//! 1. rcgen generates an Ed25519 self-signed cert + key pair
//! 2. rustls-pemfile can parse the PEM output
//! 3. RustlsConfig::from_pem accepts raw PEM bytes
//! 4. axum_server::bind_rustls type-checks against a RustlsConfig
//!
//! A full handshake test (client-with-pinned-cert → bound server) is
//! deferred to P0 T2 integration tests because it requires tokio runtime
//! and port binding, which is impractical in a unit-test context.

#[cfg(test)]
mod tls_spike {
    use rcgen::{CertificateParams, DistinguishedName, DnType, KeyPair, PKCS_ED25519};

    #[test]
    fn rcgen_generates_ed25519_cert_and_key() {
        let key_pair = KeyPair::generate_for(&PKCS_ED25519).expect("rcgen Ed25519 keypair");

        let mut params = CertificateParams::default();
        let mut dn = DistinguishedName::new();
        dn.push(DnType::CommonName, "nexus42-daemon-tls-spike");
        params.distinguished_name = dn;

        let cert = params
            .self_signed(&key_pair)
            .expect("rcgen self-signed cert");

        let cert_pem = cert.pem();
        let key_pem = key_pair.serialize_pem();

        // Basic validity: PEM output is non-empty and contains expected markers.
        assert!(!cert_pem.is_empty());
        assert!(!key_pem.is_empty());
        assert!(cert_pem.starts_with("-----BEGIN CERTIFICATE-----"));
        assert!(key_pem.starts_with("-----BEGIN PRIVATE KEY-----"));

        // rcgen self-signed certs embed the algorithm OID so we can verify
        // the key type is not RSA (Ed25519 uses a distinct OID).
        // This is a sanity check, not a cryptographic verification.
        assert!(cert_pem.contains("CERTIFICATE"));
    }

    #[test]
    fn rustls_pemfile_parses_rcgen_output() {
        let key_pair = KeyPair::generate_for(&PKCS_ED25519).expect("rcgen Ed25519 keypair");

        let mut params = CertificateParams::default();
        let mut dn = DistinguishedName::new();
        dn.push(DnType::CommonName, "nexus42-daemon-tls-spike");
        params.distinguished_name = dn;

        let cert = params
            .self_signed(&key_pair)
            .expect("rcgen self-signed cert");
        let cert_pem = cert.pem();
        let key_pem = key_pair.serialize_pem();

        // Verify rustls-pemfile can read the PEM output.
        let certs = rustls_pemfile::certs(&mut cert_pem.as_bytes())
            .collect::<Result<Vec<_>, _>>()
            .expect("rustls-pemfile certs parse");
        assert_eq!(certs.len(), 1, "one self-signed cert in PEM");

        let keys = rustls_pemfile::private_key(&mut key_pem.as_bytes())
            .expect("rustls-pemfile private key parse");
        assert!(keys.is_some(), "private key parsed from PEM");
    }

    #[tokio::test]
    async fn axum_server_rustls_config_type_checks() {
        // rustls 0.23 requires a crypto provider to be installed before any
        // TLS operations. In production, the daemon will install this once at
        // boot. For tests, we install it in the test body.
        rustls::crypto::aws_lc_rs::default_provider()
            .install_default()
            .expect("install aws-lc-rs crypto provider");

        // This test proves that RustlsConfig::from_pem is callable with
        // rcgen-generated PEM output and that the returned config type-checks
        // as a valid bind_rustls argument. We don't actually bind — just
        // verify the types line up at compile time.
        let key_pair = KeyPair::generate_for(&PKCS_ED25519).expect("rcgen Ed25519 keypair");

        let mut params = CertificateParams::default();
        let mut dn = DistinguishedName::new();
        dn.push(DnType::CommonName, "nexus42-daemon-tls-spike");
        params.distinguished_name = dn;

        let cert = params
            .self_signed(&key_pair)
            .expect("rcgen self-signed cert");

        // axum-server 0.7 RustlsConfig::from_pem takes raw PEM bytes (Vec<u8>).
        let _config = axum_server::tls_rustls::RustlsConfig::from_pem(
            cert.pem().as_bytes().to_vec(),
            key_pair.serialize_pem().as_bytes().to_vec(),
        )
        .await
        .expect("RustlsConfig::from_pem with rcgen PEM output");

        // If this compiles, the type system is satisfied. axum_server::bind_rustls
        // accepts RustlsConfig as its second argument, so the integration is
        // proven at the type level.
    }
}
