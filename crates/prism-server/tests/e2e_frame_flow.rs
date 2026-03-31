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

/// Noise IK handshake completes successfully over a real loopback QUIC
/// bidirectional stream, and the server recovers the client's static key.
#[tokio::test]
async fn noise_ik_handshake_over_quic_bi_stream() {
    use prism_security::handshake::{ClientHandshake, ServerHandshake};
    use prism_security::identity::LocalIdentity;

    let server_id = LocalIdentity::generate("Test Server");
    let client_id = LocalIdentity::generate("Test Client");

    let server_pubkey = server_id.x25519_public_bytes();
    let client_pubkey = client_id.x25519_public_bytes();

    // Build a dedicated loopback pair, keeping the client endpoint alive for
    // the duration of the test so bidirectional streams are not torn down.
    let cert = SelfSignedCert::generate().expect("cert generation must succeed");
    let cert_der_for_client = cert.cert_der.clone();
    let acceptor = ConnectionAcceptor::bind("127.0.0.1:0".parse().unwrap(), cert)
        .expect("server must bind");
    let server_addr = acceptor.local_addr();

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

    let connect_future = client_endpoint
        .connect(server_addr, "localhost")
        .expect("connect must initiate");
    let incoming = acceptor.accept().await.expect("server must accept a connection");

    let (server_conn, client_conn) = tokio::join!(
        async { incoming.await.expect("server handshake must succeed") },
        async { connect_future.await.expect("client handshake must succeed") },
    );

    // Wrap both connections in Arc so neither is dropped while the other side
    // is still using the QUIC connection.
    let server_conn = Arc::new(server_conn);
    let client_conn = Arc::new(client_conn);

    // Run client and server sides concurrently — they need to exchange messages.
    let server_conn_task = server_conn.clone();
    let server_task = tokio::spawn(async move {
        let (mut send, mut recv) = timeout(
            Duration::from_secs(2),
            server_conn_task.accept_bi(),
        )
        .await
        .expect("accept_bi must not time out")
        .expect("accept_bi must succeed");

        let client_msg = timeout(Duration::from_secs(2), recv.read_to_end(4096))
            .await
            .expect("read initiator must not time out")
            .expect("read initiator must succeed");

        let mut hs = ServerHandshake::new(&server_id).expect("ServerHandshake::new must succeed");
        let response = hs.respond(&client_msg).expect("respond must succeed");

        send.write_all(&response).await.expect("write response must succeed");
        let _ = send.finish();

        let result = hs.finalize().expect("finalize must succeed");
        result.remote_static.expect("server must have client static key")
    });

    let client_conn_task = client_conn.clone();
    let client_task = tokio::spawn(async move {
        let (mut send, mut recv) = timeout(
            Duration::from_secs(2),
            client_conn_task.open_bi(),
        )
        .await
        .expect("open_bi must not time out")
        .expect("open_bi must succeed");

        let mut hs = ClientHandshake::new(&client_id, &server_pubkey)
            .expect("ClientHandshake::new must succeed");
        let init_msg = hs.initiate().expect("initiate must succeed");

        send.write_all(&init_msg).await.expect("write init must succeed");
        let _ = send.finish();

        let server_response = timeout(Duration::from_secs(2), recv.read_to_end(4096))
            .await
            .expect("read response must not time out")
            .expect("read response must succeed");

        hs.process_response(&server_response).expect("process_response must succeed");
        hs.finalize().expect("finalize must succeed");
    });

    let (server_result, client_result) = tokio::join!(server_task, client_task);
    let recovered_client_key = server_result.expect("server task must not panic");
    client_result.expect("client task must not panic");

    assert_eq!(
        recovered_client_key, client_pubkey,
        "server must recover the exact client public key"
    );
}

/// An input datagram built by [`prism_client::InputSender`] arrives at the server
/// with the correct channel ID and deserialises to the expected [`InputEvent`].
#[tokio::test]
async fn input_datagram_reaches_server() {
    use prism_protocol::header::HEADER_SIZE;
    use prism_protocol::input::{InputEvent, INPUT_EVENT_SIZE};

    let (server_conn, client_conn) = make_loopback_pair().await;

    // Build input datagram on the client side.
    let mut input_sender = prism_client::InputSender::new();
    let datagram = input_sender.build_datagram(
        InputEvent::KeyDown { scancode: 0x1E, vk: 0x41 },
    );

    // Send client → server.
    client_conn
        .send_datagram(bytes::Bytes::copy_from_slice(&datagram))
        .expect("send_datagram must succeed");

    // Server receives within 2 seconds.
    let received = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        server_conn.read_datagram(),
    )
    .await
    .expect("read must not time out")
    .expect("read_datagram must succeed");

    // Verify the datagram is large enough and has the right channel.
    assert!(
        received.len() >= HEADER_SIZE + INPUT_EVENT_SIZE,
        "datagram must be at least header + event bytes"
    );
    let header = prism_protocol::header::PrismHeader::decode_from_slice(&received)
        .expect("header must decode");
    assert_eq!(
        header.channel_id,
        prism_protocol::channel::CHANNEL_INPUT,
        "channel must be INPUT"
    );

    // Deserialise the input event and confirm it matches.
    let event = InputEvent::from_bytes(&received[HEADER_SIZE..])
        .expect("InputEvent must parse");
    assert!(
        matches!(event, InputEvent::KeyDown { scancode: 0x1E, vk: 0x41 }),
        "event must be KeyDown {{ scancode: 0x1E, vk: 0x41 }}, got {event:?}"
    );
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
