// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2025-2026 Ehsan Khalid. All rights reserved.
// Licensed under the GNU Affero General Public License v3.0.
// Commercial licensing available — see LICENSE-COMMERCIAL.md.

//! QUIC connection acceptor with self-signed TLS.
//!
//! [`ConnectionAcceptor`] wraps a quinn server endpoint and provides a simple
//! blocking-style `accept()` call.  [`SelfSignedCert`] is a convenience type
//! for generating a localhost certificate suitable for tests and dev mode.

use std::net::SocketAddr;
use std::sync::Arc;

use thiserror::Error;

use prism_transport::quic::config::latency_transport_config;

// ── Error ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum AcceptorError {
    #[error("TLS certificate error: {0}")]
    Tls(String),
    #[error("endpoint bind error: {0}")]
    Bind(#[from] std::io::Error),
}

// ── SelfSignedCert ────────────────────────────────────────────────────────────

/// A DER-encoded self-signed certificate + private key pair.
///
/// Use [`SelfSignedCert::generate`] to create one for `"localhost"`.
pub struct SelfSignedCert {
    /// DER-encoded X.509 certificate.
    pub cert_der: rustls::pki_types::CertificateDer<'static>,
    /// DER-encoded PKCS#8 private key.
    pub key_der: rustls::pki_types::PrivateKeyDer<'static>,
}

impl SelfSignedCert {
    /// Generate a new self-signed certificate for `"localhost"`.
    pub fn generate() -> Result<Self, AcceptorError> {
        let rcgen::CertifiedKey { cert, key_pair } =
            rcgen::generate_simple_self_signed(vec!["localhost".to_string()])
                .map_err(|e| AcceptorError::Tls(e.to_string()))?;

        let cert_der = cert.der().clone();
        let key_der = rustls::pki_types::PrivateKeyDer::Pkcs8(
            rustls::pki_types::PrivatePkcs8KeyDer::from(key_pair.serialize_der()),
        );

        Ok(Self { cert_der, key_der })
    }
}

// ── ConnectionAcceptor ────────────────────────────────────────────────────────

/// Listens for incoming QUIC connections and hands them to the session manager.
pub struct ConnectionAcceptor {
    endpoint: quinn::Endpoint,
    local_addr: SocketAddr,
}

impl ConnectionAcceptor {
    /// Bind a QUIC server endpoint to `addr` using the given TLS certificate.
    ///
    /// Uses [`latency_transport_config`] for transport tuning.
    pub fn bind(addr: SocketAddr, tls_cert: SelfSignedCert) -> Result<Self, AcceptorError> {
        let mut server_config =
            quinn::ServerConfig::with_single_cert(
                vec![tls_cert.cert_der],
                tls_cert.key_der,
            )
            .map_err(|e| AcceptorError::Tls(e.to_string()))?;

        server_config
            .transport_config(Arc::new(latency_transport_config(None)));

        let endpoint = quinn::Endpoint::server(server_config, addr)?;
        let local_addr = endpoint.local_addr()?;

        Ok(Self { endpoint, local_addr })
    }

    /// Return the local address the endpoint is bound to.
    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    /// Wait for the next incoming connection.
    ///
    /// Returns `None` when the endpoint has been closed.
    pub async fn accept(&self) -> Option<quinn::Incoming> {
        self.endpoint.accept().await
    }

    /// Close the endpoint, rejecting all pending and future connections.
    pub fn close(&self) {
        self.endpoint.close(0u32.into(), b"server shutdown");
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn self_signed_cert_generates() {
        let cert = SelfSignedCert::generate().expect("cert generation must succeed");
        assert!(!cert.cert_der.is_empty(), "cert_der must be non-empty");
    }

    #[tokio::test]
    async fn acceptor_binds_to_port() {
        let cert = SelfSignedCert::generate().unwrap();
        let acceptor =
            ConnectionAcceptor::bind("127.0.0.1:0".parse().unwrap(), cert).unwrap();
        assert!(acceptor.local_addr().port() > 0, "must bind to a real port");
        acceptor.close();
    }

    #[tokio::test]
    async fn acceptor_accepts_connection() {
        // ── server ───────────────────────────────────────────────────────────
        let cert = SelfSignedCert::generate().unwrap();
        // Clone the cert_der before moving into acceptor.
        let cert_der_for_client = cert.cert_der.clone();

        let acceptor =
            ConnectionAcceptor::bind("127.0.0.1:0".parse().unwrap(), cert).unwrap();
        let server_addr = acceptor.local_addr();

        // ── client ───────────────────────────────────────────────────────────
        let mut roots = rustls::RootCertStore::empty();
        roots.add(cert_der_for_client).unwrap();

        let client_crypto = rustls::ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();
        let client_config = quinn::ClientConfig::new(Arc::new(
            quinn::crypto::rustls::QuicClientConfig::try_from(client_crypto).unwrap(),
        ));

        let mut client_endpoint =
            quinn::Endpoint::client("0.0.0.0:0".parse().unwrap()).unwrap();
        client_endpoint.set_default_client_config(client_config);

        // ── handshake ────────────────────────────────────────────────────────
        let connect_future = client_endpoint.connect(server_addr, "localhost").unwrap();
        let incoming = acceptor.accept().await.expect("server must accept");

        let (client_conn, server_conn) = tokio::join!(
            async { connect_future.await.unwrap() },
            async { incoming.await.unwrap() },
        );

        // Both sides connected — verify the client sees the server address and
        // the server sees a remote (the client's ephemeral address).
        assert_eq!(client_conn.remote_address(), server_addr);
        assert!(server_conn.remote_address().port() > 0);

        client_conn.close(0u32.into(), b"done");
        acceptor.close();
    }
}
