//! QUIC client connector with configurable TLS verification.
//!
//! [`ClientConnector`] wraps a [`quinn::Endpoint`] and supports two TLS modes:
//! - [`TlsMode::InsecureTrustAll`]: accepts any server certificate (dev/test only).
//! - [`TlsMode::TrustCert`]: trusts a specific DER-encoded certificate.

use std::net::SocketAddr;
use std::sync::Arc;

use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified};
use rustls::{DigitallySignedStruct, Error as RustlsError, SignatureScheme};
use thiserror::Error;

// ── Error ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum ConnectorError {
    #[error("TLS configuration error: {0}")]
    Tls(String),
    #[error("endpoint bind error: {0}")]
    Bind(#[from] std::io::Error),
    #[error("connection error: {0}")]
    Connect(#[from] quinn::ConnectError),
    #[error("connection failed: {0}")]
    ConnectionFailed(#[from] quinn::ConnectionError),
}

// ── TlsMode ───────────────────────────────────────────────────────────────────

/// Controls how the client verifies the server's TLS certificate.
#[derive(Debug, Clone)]
pub enum TlsMode {
    /// Accept any server certificate without verification. **Dev/test only.**
    InsecureTrustAll,
    /// Trust a single specific DER-encoded X.509 certificate.
    TrustCert(CertificateDer<'static>),
}

// ── InsecureCertVerifier ──────────────────────────────────────────────────────

/// A `ServerCertVerifier` that blindly accepts every certificate presented.
///
/// **Security warning:** this disables all certificate validation and must only
/// be used in controlled development or testing environments.
#[derive(Debug)]
struct InsecureCertVerifier;

impl rustls::client::danger::ServerCertVerifier for InsecureCertVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, RustlsError> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, RustlsError> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, RustlsError> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        // Return all well-known schemes so the TLS handshake can proceed with
        // whatever scheme the server chooses.
        vec![
            SignatureScheme::RSA_PKCS1_SHA1,
            SignatureScheme::ECDSA_SHA1_Legacy,
            SignatureScheme::RSA_PKCS1_SHA256,
            SignatureScheme::ECDSA_NISTP256_SHA256,
            SignatureScheme::RSA_PKCS1_SHA384,
            SignatureScheme::ECDSA_NISTP384_SHA384,
            SignatureScheme::RSA_PKCS1_SHA512,
            SignatureScheme::ECDSA_NISTP521_SHA512,
            SignatureScheme::RSA_PSS_SHA256,
            SignatureScheme::RSA_PSS_SHA384,
            SignatureScheme::RSA_PSS_SHA512,
            SignatureScheme::ED25519,
            SignatureScheme::ED448,
        ]
    }
}

// ── ClientConnector ───────────────────────────────────────────────────────────

/// QUIC client endpoint that can connect to a PRISM server.
pub struct ClientConnector {
    endpoint: quinn::Endpoint,
}

impl ClientConnector {
    /// Create a new client endpoint with the given TLS verification mode.
    ///
    /// Binds an ephemeral UDP port on `0.0.0.0:0`.
    pub fn new(tls_mode: TlsMode) -> Result<Self, ConnectorError> {
        let client_crypto = build_client_crypto(tls_mode)?;

        let client_config = quinn::ClientConfig::new(Arc::new(
            quinn::crypto::rustls::QuicClientConfig::try_from(client_crypto)
                .map_err(|e| ConnectorError::Tls(e.to_string()))?,
        ));

        let mut endpoint =
            quinn::Endpoint::client("0.0.0.0:0".parse::<SocketAddr>().unwrap())?;
        endpoint.set_default_client_config(client_config);

        Ok(Self { endpoint })
    }

    /// Connect to a PRISM server at `addr` with the given SNI `server_name`.
    pub async fn connect(
        &self,
        addr: SocketAddr,
        server_name: &str,
    ) -> Result<quinn::Connection, ConnectorError> {
        let connecting = self.endpoint.connect(addr, server_name)?;
        let conn = connecting.await?;
        Ok(conn)
    }

    /// Close the underlying QUIC endpoint.
    pub fn close(&self) {
        self.endpoint.close(0u32.into(), b"client shutdown");
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn build_client_crypto(tls_mode: TlsMode) -> Result<rustls::ClientConfig, ConnectorError> {
    match tls_mode {
        TlsMode::InsecureTrustAll => {
            let config = rustls::ClientConfig::builder()
                .dangerous()
                .with_custom_certificate_verifier(Arc::new(InsecureCertVerifier))
                .with_no_client_auth();
            Ok(config)
        }
        TlsMode::TrustCert(cert_der) => {
            let mut roots = rustls::RootCertStore::empty();
            roots
                .add(cert_der)
                .map_err(|e| ConnectorError::Tls(e.to_string()))?;
            let config = rustls::ClientConfig::builder()
                .with_root_certificates(roots)
                .with_no_client_auth();
            Ok(config)
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn insecure_connector_creates() {
        let connector = ClientConnector::new(TlsMode::InsecureTrustAll)
            .expect("insecure connector must create without error");
        // Verify the connector exists (endpoint is live)
        connector.close();
    }
}
