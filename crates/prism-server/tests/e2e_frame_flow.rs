//! End-to-end integration tests: server → client display frame delivery over
//! loopback QUIC.
//!
//! Each test stands up a real `ConnectionAcceptor` bound to `127.0.0.1:0`,
//! connects a raw quinn client endpoint that trusts the self-signed cert, and
//! exchanges datagrams.  All network I/O is wrapped in a 2-second timeout so
//! the test suite never hangs.

use std::sync::Arc;
use std::time::Duration;

use tokio::time::timeout;

use prism_protocol::channel::CHANNEL_DISPLAY;
use prism_protocol::header::{PrismHeader, HEADER_SIZE};
use prism_server::{ConnectionAcceptor, SelfSignedCert, build_display_datagram};

// ── Loopback pair helper ──────────────────────────────────────────────────────

/// Spin up a server endpoint and a client endpoint that trusts its self-signed
/// cert.  Returns `(server_conn, client_conn)` after completing the TLS
/// handshake.
async fn make_loopback_pair() -> (quinn::Connection, quinn::Connection) {
    // Generate cert; clone cert_der before moving SelfSignedCert into the acceptor.
    let cert = SelfSignedCert::generate().expect("cert generation must succeed");
    let cert_der_for_client = cert.cert_der.clone();

    let acceptor = ConnectionAcceptor::bind("127.0.0.1:0".parse().unwrap(), cert)
        .expect("server must bind");
    let server_addr = acceptor.local_addr();

    // Build a client endpoint that trusts only the server's specific cert.
    let mut roots = rustls::RootCertStore::empty();
    roots.add(cert_der_for_client).expect("root cert add must succeed");

    let client_crypto = rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    let client_config = quinn::ClientConfig::new(Arc::new(
        quinn::crypto::rustls::QuicClientConfig::try_from(client_crypto)
            .expect("QuicClientConfig must build"),
    ));

    let mut client_endpoint =
        quinn::Endpoint::client("0.0.0.0:0".parse().unwrap()).expect("client endpoint must bind");
    client_endpoint.set_default_client_config(client_config);

    // Race accept + connect; both must complete.
    let connect_future = client_endpoint
        .connect(server_addr, "localhost")
        .expect("connect must initiate");
    let incoming = acceptor.accept().await.expect("server must accept a connection");

    let (client_conn, server_conn) = tokio::join!(
        async { connect_future.await.expect("client handshake must succeed") },
        async { incoming.await.expect("server handshake must succeed") },
    );

    // We don't close the acceptor here — the caller owns the connections.
    // Dropping `acceptor` is fine; existing connections stay alive.
    (server_conn, client_conn)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

/// A single display datagram sent by the server arrives at the client with the
/// correct `channel_id` and `sequence` fields.
#[tokio::test]
async fn server_sends_datagram_client_receives() {
    let (server_conn, client_conn) = make_loopback_pair().await;

    let payload = b"test_frame";
    let dgram = build_display_datagram(0, payload, 1000);

    server_conn
        .send_datagram(dgram)
        .expect("send_datagram must succeed");

    let received = timeout(Duration::from_secs(2), client_conn.read_datagram())
        .await
        .expect("read must not time out")
        .expect("read_datagram must succeed");

    assert!(
        received.len() >= HEADER_SIZE,
        "datagram must be at least HEADER_SIZE bytes"
    );
    let header = PrismHeader::decode_from_slice(&received[..HEADER_SIZE])
        .expect("header must decode");

    assert_eq!(header.channel_id, CHANNEL_DISPLAY, "channel must be DISPLAY");
    assert_eq!(header.sequence, 0, "sequence must match");
}

/// Ten sequential display datagrams sent by the server all arrive at the client
/// and every expected sequence number is present exactly once.
#[tokio::test]
async fn multiple_frames_arrive() {
    let (server_conn, client_conn) = make_loopback_pair().await;

    const N: u32 = 10;

    // Send all frames before reading any — QUIC datagrams are fire-and-forget.
    for seq in 0..N {
        let dgram = build_display_datagram(seq, b"frame_data", 0);
        server_conn
            .send_datagram(dgram)
            .expect("send_datagram must succeed");
    }

    let mut received_seqs = Vec::with_capacity(N as usize);

    for _ in 0..N {
        let data = timeout(Duration::from_secs(2), client_conn.read_datagram())
            .await
            .expect("read must not time out")
            .expect("read_datagram must succeed");

        assert!(data.len() >= HEADER_SIZE, "datagram must contain full header");
        let header = PrismHeader::decode_from_slice(&data[..HEADER_SIZE])
            .expect("header must decode");

        assert_eq!(header.channel_id, CHANNEL_DISPLAY);
        received_seqs.push(header.sequence);
    }

    // All 10 sequences present (order may vary over UDP datagrams).
    received_seqs.sort_unstable();
    let expected: Vec<u32> = (0..N).collect();
    assert_eq!(received_seqs, expected, "all sequences 0–9 must arrive");
}

/// Unit-level smoke test: `build_display_datagram` produces a buffer whose
/// decoded header has the correct channel, sequence, and message type.
#[tokio::test]
async fn frame_sender_builds_valid_datagrams() {
    let payload = b"pixel_data_here";
    let dgram = build_display_datagram(42, payload, 99_999);

    assert!(
        dgram.len() >= HEADER_SIZE,
        "datagram must be at least HEADER_SIZE bytes"
    );

    let header = PrismHeader::decode_from_slice(&dgram[..HEADER_SIZE])
        .expect("header must decode");

    assert_eq!(header.channel_id, CHANNEL_DISPLAY, "channel must be DISPLAY");
    assert_eq!(header.sequence, 42, "sequence must match");
    assert_eq!(header.timestamp_us, 99_999, "timestamp must match");
    // MSG_TYPE_SLICE = 0x02
    assert_eq!(header.msg_type, 0x02, "msg_type must be SLICE (0x02)");
    assert!(
        header.payload_length as usize >= payload.len(),
        "payload_length must cover the payload"
    );
}
