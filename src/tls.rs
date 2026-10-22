//! The HTTPS client every outbound request is built from.
//!
//! It trusts the same roots reqwest would on its own (the bundled Mozilla set
//! plus the operating system's store) and additionally refuses any chain that
//! presents one of the eMudhra intermediate CA keys disclosed in October 2026.
//! Those keys can sign a certificate for any name, and neither root bundles
//! nor this client check revocation, so a store that still holds
//! `AAA Certificate Services` or `emSign ECC Root CA - G3` would otherwise
//! accept such a certificate.

use std::sync::{Arc, OnceLock};

use anyhow::{Context, Result, anyhow};
use rustls::client::WebPkiServerVerifier;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::CryptoProvider;
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{
    CertificateError, ClientConfig, DigitallySignedStruct, RootCertStore, SignatureScheme,
};
use sha2::{Digest, Sha256};

/// SHA-256 of the DER `SubjectPublicKeyInfo` of the eight disclosed eMudhra
/// intermediate keys (GO-2026-6809). The list is the one Go 1.26.10 and
/// 1.27.3 enforce in `crypto/x509`.
const DISTRUSTED_SPKI_SHA256: [&str; 8] = [
    "ebed6475c2c2d80fcb199f3fe8dd6e6b3ce60b77eb15d4e42c3dec0c6f5b5105",
    "49dd353dc2b919a3770645117d0e60c16115e7d1bc259eefe734b00ea434fe5f",
    "e0254c7fcdc265e2f0f8a428dabb75ab4ae039b4872d62f5ee57f0877d5a8a41",
    "5c0660a2b164910d8ff589c18e14552e9e0ec237dcefe2cb736d41a4811f81a0",
    "43bffa19d6ef651451f6841e019931e96f6f2eaeff28ab4b6f1ce4576de2717b",
    "63ecefee431d49523bc1136c4e72d0206a477086caf606cd5cf025686c109cf8",
    "ec4ffa1ad0ac88081314b7aaffef6a91f97cd57291bb5ba686c5415f5eb4f6b6",
    "92c063d67e8f14db71d288e11266ffdaf69cbf115f7bab14f3bc2f68d25e0e30",
];

/// A client builder with cerul's TLS policy applied. Callers add their own
/// timeouts, proxy and redirect settings.
pub fn client_builder() -> Result<reqwest::ClientBuilder> {
    static CONFIG: OnceLock<std::result::Result<ClientConfig, String>> = OnceLock::new();
    let config = CONFIG
        .get_or_init(|| client_config().map_err(|error| format!("{error:#}")))
        .as_ref()
        .map_err(|error| anyhow!("{error}"))?;
    Ok(reqwest::Client::builder().use_preconfigured_tls(config.clone()))
}

fn client_config() -> Result<ClientConfig> {
    let provider = CryptoProvider::get_default()
        .cloned()
        .unwrap_or_else(|| Arc::new(rustls::crypto::ring::default_provider()));
    let mut roots = RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    // Native stores often hold certificates rustls cannot parse; skip them as
    // reqwest does instead of failing.
    for cert in rustls_native_certs::load_native_certs().certs {
        let _ = roots.add(cert);
    }
    let inner = WebPkiServerVerifier::builder_with_provider(Arc::new(roots), provider.clone())
        .build()
        .context("could not build the certificate verifier")?;
    let mut config = ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .context("could not select TLS versions")?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(DistrustingVerifier { inner }))
        .with_no_client_auth();
    config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
    Ok(config)
}

/// Delegates to the standard WebPKI verifier after rejecting any presented
/// certificate whose key is on the distrust list.
#[derive(Debug)]
struct DistrustingVerifier {
    inner: Arc<WebPkiServerVerifier>,
}

impl ServerCertVerifier for DistrustingVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        server_name: &ServerName<'_>,
        ocsp_response: &[u8],
        now: UnixTime,
    ) -> std::result::Result<ServerCertVerified, rustls::Error> {
        if std::iter::once(end_entity)
            .chain(intermediates)
            .any(is_distrusted)
        {
            return Err(rustls::Error::InvalidCertificate(CertificateError::Revoked));
        }
        self.inner
            .verify_server_cert(end_entity, intermediates, server_name, ocsp_response, now)
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        self.inner.verify_tls12_signature(message, cert, dss)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        self.inner.verify_tls13_signature(message, cert, dss)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.inner.supported_verify_schemes()
    }
}

fn is_distrusted(cert: &CertificateDer<'_>) -> bool {
    spki_sha256(cert).is_some_and(|hash| DISTRUSTED_SPKI_SHA256.contains(&hash.as_str()))
}

/// Hex SHA-256 of a certificate's full DER `SubjectPublicKeyInfo`. A
/// certificate that does not parse is left to the WebPKI verifier to reject.
fn spki_sha256(cert: &CertificateDer<'_>) -> Option<String> {
    let anchor = webpki::anchor_from_trusted_cert(cert).ok()?;
    // webpki keeps the field's contents; the hash covers the whole SEQUENCE.
    let contents = anchor.subject_public_key_info.as_ref();
    let mut spki = vec![0x30];
    push_der_length(&mut spki, contents.len());
    spki.extend_from_slice(contents);
    Some(crate::storage::hex(Sha256::digest(&spki)))
}

fn push_der_length(out: &mut Vec<u8>, length: usize) {
    if length < 0x80 {
        out.push(length as u8);
        return;
    }
    let bytes = length.to_be_bytes();
    let significant = &bytes[bytes
        .iter()
        .position(|&byte| byte != 0)
        .unwrap_or(bytes.len() - 1)..];
    out.push(0x80 | significant.len() as u8);
    out.extend_from_slice(significant);
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIRECT: &[u8] = include_bytes!("../tests/fixtures/emudhra-dv-tls-g3b.der");
    const CROSS_SIGNED: &[u8] = include_bytes!("../tests/fixtures/emudhra-dv-tls-g3b-aaa.der");
    const DIRECT_SPKI_SHA256: &str =
        "ebed6475c2c2d80fcb199f3fe8dd6e6b3ce60b77eb15d4e42c3dec0c6f5b5105";

    #[test]
    fn both_issuances_of_a_disclosed_intermediate_are_distrusted() {
        for der in [DIRECT, CROSS_SIGNED] {
            let cert = CertificateDer::from(der);
            assert_eq!(spki_sha256(&cert).as_deref(), Some(DIRECT_SPKI_SHA256));
            assert!(is_distrusted(&cert));
        }
    }

    #[test]
    fn ordinary_roots_are_not_distrusted() {
        let roots = rustls_native_certs::load_native_certs().certs;
        let parsed = roots.iter().filter_map(spki_sha256).count();
        assert!(parsed > 0 || roots.is_empty());
        assert!(!roots.iter().any(is_distrusted));
    }

    #[test]
    fn a_chain_presenting_a_disclosed_intermediate_is_refused_before_path_building() {
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let mut roots = RootCertStore::empty();
        roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        let inner = WebPkiServerVerifier::builder_with_provider(Arc::new(roots), provider)
            .build()
            .expect("verifier");
        let verifier = DistrustingVerifier { inner };
        let leaf = CertificateDer::from(DIRECT);
        let error = verifier
            .verify_server_cert(
                &leaf,
                &[CertificateDer::from(CROSS_SIGNED)],
                &ServerName::try_from("example.com").expect("name"),
                &[],
                UnixTime::now(),
            )
            .expect_err("distrusted chain");
        assert_eq!(
            error,
            rustls::Error::InvalidCertificate(CertificateError::Revoked)
        );
    }

    #[test]
    fn der_lengths_use_the_shortest_form() {
        for (length, encoded) in [
            (0x76, vec![0x76]),
            (0x80, vec![0x81, 0x80]),
            (0x0222, vec![0x82, 0x02, 0x22]),
        ] {
            let mut out = Vec::new();
            push_der_length(&mut out, length);
            assert_eq!(out, encoded);
        }
    }
}
