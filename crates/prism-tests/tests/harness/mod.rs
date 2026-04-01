// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2025-2026 Ehsan Khalid. All rights reserved.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use prism_server::{ServerApp, ServerConfig};

// ── TestServer ──────────────────────────────────────────────────────────────

/// A PRISM server running in-process with test-friendly defaults.
pub struct TestServer {
    addr: SocketAddr,
    shutdown_tx: tokio::sync::watch::Sender<bool>,
    cert_der: rustls::pki_types::CertificateDer<'static>,
    task: tokio::task::JoinHandle<()>,
}

impl TestServer {
    /// Start a test server with default configuration.
    pub async fn start() -> Self {
        Self::start_with(false).await
    }

    /// Start a test server. If `noise_mode` is true, Noise IK is enabled.
    pub async fn start_with(noise_mode: bool) -> Self {
        let config = ServerConfig {
            listen_addr_str: "127.0.0.1:0".to_string(),
            throughput_addr_str: "127.0.0.1:0".to_string(),
            heartbeat_suspend_secs: 2,
            heartbeat_tombstone_secs: 5,
            tombstone_max_age_secs: 10,
            max_clients: 4,
            ..ServerConfig::default()
        };

        let mut app = ServerApp::with_config(false, noise_mode, 0, config)
            .expect("TestServer must construct");

        let cert_der = app.cert_der();
        let shutdown_tx = app.shutdown_tx();

        let (addr_tx, addr_rx) = tokio::sync::oneshot::channel();
        app.set_bound_addr_notify(addr_tx);

        let task = tokio::spawn(async move {
            if let Err(e) = app.run().await {
                tracing::error!(error = %e, "TestServer::run() error");
            }
        });

        let addr = addr_rx.await.expect("TestServer must send bound address");

        Self { addr, shutdown_tx, cert_der, task }
    }

    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    pub fn cert_der(&self) -> &rustls::pki_types::CertificateDer<'static> {
        &self.cert_der
    }

    pub async fn shutdown(self) {
        let _ = self.shutdown_tx.send(true);
        let _ = tokio::time::timeout(Duration::from_secs(5), self.task).await;
    }
}

// ── TestClient ──────────────────────────────────────────────────────────────

/// A lightweight QUIC client that trusts a specific server certificate.
/// Does NOT open a window — suitable for headless CI.
pub struct TestClient {
    connection: quinn::Connection,
    endpoint: quinn::Endpoint,
}

impl TestClient {
    pub async fn connect(server: &TestServer) -> Self {
        Self::connect_to(server.addr(), server.cert_der()).await
    }

    pub async fn connect_to(
        addr: SocketAddr,
        cert_der: &rustls::pki_types::CertificateDer<'static>,
    ) -> Self {
        let mut roots = rustls::RootCertStore::empty();
        roots.add(cert_der.clone()).expect("root cert add must succeed");

        let client_crypto = rustls::ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();
        let client_config = quinn::ClientConfig::new(Arc::new(
            quinn::crypto::rustls::QuicClientConfig::try_from(client_crypto)
                .expect("QuicClientConfig must build"),
        ));

        let mut endpoint = quinn::Endpoint::client("0.0.0.0:0".parse().unwrap())
            .expect("client endpoint must bind");
        endpoint.set_default_client_config(client_config);

        let connection = endpoint
            .connect(addr, "localhost")
            .expect("connect must initiate")
            .await
            .expect("client handshake must succeed");

        Self { connection, endpoint }
    }

    pub fn connection(&self) -> &quinn::Connection {
        &self.connection
    }

    pub fn send_datagram(&self, data: bytes::Bytes) {
        self.connection
            .send_datagram(data)
            .expect("send_datagram must succeed");
    }

    pub async fn recv_datagram(&self) -> bytes::Bytes {
        timeout_secs(10, self.connection.read_datagram())
            .await
            .expect("recv_datagram must succeed")
    }

    pub async fn open_bi(&self) -> (quinn::SendStream, quinn::RecvStream) {
        timeout_secs(10, self.connection.open_bi()).await.expect("open_bi must succeed")
    }

    pub fn close(self) {
        self.connection.close(0u32.into(), b"test done");
    }

    pub fn drop_abruptly(self) {
        drop(self.connection);
        drop(self.endpoint);
    }
}

// ── Helpers ─────────────────────────────────────────────────────────────────

pub async fn timeout_secs<F: std::future::Future>(secs: u64, future: F) -> F::Output {
    tokio::time::timeout(Duration::from_secs(secs), future)
        .await
        .unwrap_or_else(|_| panic!("operation timed out after {secs}s"))
}
