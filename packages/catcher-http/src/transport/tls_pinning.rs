//! TLS certificate pinning via SHA-256 fingerprint comparison.
//!
//! Wraps the standard `WebPkiServerVerifier` with an additional check:
//! the SHA-256 hash of the end-entity certificate's DER bytes must match
//! one of the configured pins.

use base64::Engine;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, Error as TlsError, SignatureScheme};
use sha2::{Digest, Sha256};
use std::sync::Arc;

/// A certificate verifier that enforces SHA-256 pinning on top of normal validation.
#[derive(Debug)]
pub struct PinningVerifier {
    inner: Arc<dyn ServerCertVerifier>,
    /// Decoded SHA-256 hashes (raw 32 bytes each)
    pins: Vec<Vec<u8>>,
}

impl PinningVerifier {
    /// Create a new pinning verifier wrapping `inner`.
    ///
    /// `pins` should be base64-encoded SHA-256 hashes of certificate DER bytes.
    /// Invalid base64 values are silently ignored.
    pub fn new(inner: Arc<dyn ServerCertVerifier>, pins: &[String]) -> Self {
        let decoded: Vec<Vec<u8>> = pins
            .iter()
            .filter_map(|p| {
                base64::engine::general_purpose::STANDARD
                    .decode(p.trim())
                    .ok()
            })
            .filter(|b: &Vec<u8>| b.len() == 32)
            .collect();
        Self {
            inner,
            pins: decoded,
        }
    }
}

impl ServerCertVerifier for PinningVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        server_name: &ServerName<'_>,
        ocsp_response: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, TlsError> {
        // 1. Normal chain + hostname validation
        let verified = self.inner.verify_server_cert(
            end_entity,
            intermediates,
            server_name,
            ocsp_response,
            now,
        )?;

        // 2. Pin check — at least one pin must match
        if !self.pins.is_empty() {
            let cert_hash = Sha256::digest(end_entity.as_ref()).to_vec();
            let matched = self.pins.iter().any(|pin| pin == &cert_hash);
            if !matched {
                return Err(TlsError::General(
                    "certificate pin mismatch: none of the configured SHA-256 pins match the server certificate".into(),
                ));
            }
        }

        Ok(verified)
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, TlsError> {
        self.inner.verify_tls12_signature(message, cert, dss)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, TlsError> {
        self.inner.verify_tls13_signature(message, cert, dss)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.inner.supported_verify_schemes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::OnceLock;

    /// 确保 rustls CryptoProvider 已安装（测试用）。
    /// rustls 0.23+ 需要显式安装 crypto backend。
    static CRYPTO_INSTALLED: OnceLock<()> = OnceLock::new();

    fn ensure_crypto_provider() {
        CRYPTO_INSTALLED.get_or_init(|| {
            // catcher-http 使用 aws_lc_rs feature。
            // workspace 构建中 catcher-ws 可能同时启用 ring，但 aws_lc_rs 优先。
            let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
        });
    }

    fn make_mock_verifier() -> Arc<dyn ServerCertVerifier> {
        ensure_crypto_provider();
        let mut roots = rustls::RootCertStore::empty();
        roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        rustls::client::WebPkiServerVerifier::builder(Arc::new(roots))
            .build()
            .unwrap()
    }

    #[test]
    fn pin1_decode_base64_pins() {
        let hash = sha2::Sha256::digest(b"hello world");
        let b64 = base64::engine::general_purpose::STANDARD.encode(hash);
        let v = PinningVerifier::new(make_mock_verifier(), std::slice::from_ref(&b64));
        assert_eq!(v.pins.len(), 1);
        assert_eq!(v.pins[0], hash.as_slice());
    }

    #[test]
    fn pin2_ignore_invalid_pins() {
        let v = PinningVerifier::new(
            make_mock_verifier(),
            &["not-base64!!!".to_string(), "AAAA".to_string()],
        );
        assert_eq!(v.pins.len(), 0);
    }
}
