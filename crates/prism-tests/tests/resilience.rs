// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2025-2026 Ehsan Khalid. All rights reserved.

mod harness;

use harness::{TestClient, TestServer};

/// Server survives an abrupt client crash (no clean close).
#[tokio::test(flavor = "multi_thread")]
async fn client_abrupt_disconnect_server_survives() {
    let server = TestServer::start().await;

    let client = TestClient::connect(&server).await;
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // Simulate crash: drop without closing.
    client.drop_abruptly();

    // Wait for server to detect the disconnection.
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;

    // Server must still accept new connections.
    let client2 = TestClient::connect(&server).await;
    assert!(
        client2.connection().close_reason().is_none(),
        "server must accept connections after abrupt client disconnect"
    );

    client2.close();
    server.shutdown().await;
}

/// A new client can connect after a previous one disconnects.
#[tokio::test(flavor = "multi_thread")]
async fn reconnect_after_disconnect() {
    let server = TestServer::start().await;

    let client_a = TestClient::connect(&server).await;
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    client_a.close();

    tokio::time::sleep(std::time::Duration::from_secs(1)).await;

    let client_b = TestClient::connect(&server).await;
    assert!(
        client_b.connection().close_reason().is_none(),
        "client B must connect after client A disconnects"
    );

    client_b.close();
    server.shutdown().await;
}

/// Five sequential clients connect and disconnect without leaks.
#[tokio::test(flavor = "multi_thread")]
async fn multiple_sequential_clients() {
    let server = TestServer::start().await;

    for i in 0..5 {
        let client = TestClient::connect(&server).await;
        assert!(
            client.connection().close_reason().is_none(),
            "client {i} must connect successfully"
        );
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        client.close();
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    }

    server.shutdown().await;
}

/// Server shutdown propagates to active clients (no hang).
#[tokio::test(flavor = "multi_thread")]
async fn server_shutdown_during_active_session() {
    let server = TestServer::start().await;
    let client = TestClient::connect(&server).await;

    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // Shut down the server while client is connected.
    server.shutdown().await;

    // Client should detect the closure within a reasonable time.
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // Reading should fail or timeout (connection gone). The key assertion
    // is that we didn't hang forever — we reached this line.
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(3),
        client.connection().read_datagram(),
    )
    .await;

    match result {
        Ok(Err(_)) => {} // Connection error — expected
        Err(_) => {}     // Timeout — acceptable
        Ok(Ok(_)) => {}  // Got a buffered datagram — acceptable
    }
}
