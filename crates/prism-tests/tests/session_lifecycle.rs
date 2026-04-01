// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2025-2026 Ehsan Khalid. All rights reserved.

mod harness;

use harness::{TestServer, TestClient};

/// A client connects and the QUIC session is established successfully.
#[tokio::test(flavor = "multi_thread")]
async fn connect_and_session_established() {
    let server = TestServer::start().await;
    let client = TestClient::connect(&server).await;

    // Connection should be alive.
    assert!(
        client.connection().close_reason().is_none(),
        "connection must be established"
    );

    // Wait a moment to confirm stability.
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    assert!(
        client.connection().close_reason().is_none(),
        "connection must remain stable"
    );

    client.close();
    server.shutdown().await;
}

/// An input datagram sent by the client doesn't crash the server.
#[tokio::test(flavor = "multi_thread")]
async fn input_round_trip() {
    use prism_protocol::input::InputEvent;

    let server = TestServer::start().await;
    let client = TestClient::connect(&server).await;

    // Build and send an input datagram.
    let mut input_sender = prism_client::InputSender::new();
    let datagram = input_sender.build_datagram(
        InputEvent::KeyDown { scancode: 0x1E, vk: 0x41 },
    );
    client.send_datagram(datagram);

    // Give the server time to process.
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // Connection should still be open.
    assert!(
        client.connection().close_reason().is_none(),
        "connection must remain open after sending input"
    );

    client.close();
    server.shutdown().await;
}

/// A Noise-mode server accepts QUIC connections (TLS 1.3 layer works).
#[tokio::test(flavor = "multi_thread")]
async fn noise_mode_connection() {
    let server = TestServer::start_with(true).await;
    let client = TestClient::connect(&server).await;

    // QUIC connection is established (TLS 1.3 encrypted).
    // The full Noise IK handshake requires the server's Noise public key
    // which is tested separately in e2e_frame_flow.rs.
    assert!(
        client.connection().close_reason().is_none(),
        "connection to noise-mode server must succeed"
    );

    client.close();
    server.shutdown().await;
}

/// Connection survives beyond the heartbeat suspend timeout.
#[tokio::test(flavor = "multi_thread")]
async fn heartbeat_keeps_session_alive() {
    let server = TestServer::start().await;
    let client = TestClient::connect(&server).await;

    // Server's heartbeat_suspend_secs = 2. Wait 4 seconds.
    // QUIC's idle timeout is longer, so the connection stays alive.
    tokio::time::sleep(std::time::Duration::from_secs(4)).await;

    assert!(
        client.connection().close_reason().is_none(),
        "connection must survive beyond heartbeat_suspend_secs"
    );

    client.close();
    server.shutdown().await;
}

/// Server continues accepting connections after a client disconnects cleanly.
#[tokio::test(flavor = "multi_thread")]
async fn graceful_disconnect() {
    let server = TestServer::start().await;

    // Connect and disconnect.
    let client = TestClient::connect(&server).await;
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    client.close();

    // Wait for server to process the disconnect.
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;

    // New client must connect successfully.
    let client2 = TestClient::connect(&server).await;
    assert!(
        client2.connection().close_reason().is_none(),
        "server must accept new connections after a client disconnects"
    );

    client2.close();
    server.shutdown().await;
}
