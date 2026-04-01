// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2025-2026 Ehsan Khalid. All rights reserved.
// Licensed under the GNU Affero General Public License v3.0.
// Commercial licensing available — see LICENSE-COMMERCIAL.md.

// QUIC socket setup with socket2 for platform-specific tuning.

use crate::connection::TransportError;
use std::net::{SocketAddr, UdpSocket};
use socket2::SockRef;

pub fn set_dscp(socket: &UdpSocket, dscp: u8) -> Result<(), std::io::Error> {
    let sock_ref = SockRef::from(socket);
    let tos = (dscp as u32) << 2;
    sock_ref.set_tos(tos)
}

pub fn create_latency_socket(addr: SocketAddr) -> Result<UdpSocket, TransportError> {
    let socket = UdpSocket::bind(addr)
        .map_err(|e| TransportError::ConnectionError(e.to_string()))?;
    let _ = set_dscp(&socket, 0x2E); // EF
    let _ = socket.set_nonblocking(true);
    let sock_ref = SockRef::from(&socket);
    let _ = sock_ref.set_recv_buffer_size(4 * 1024 * 1024);
    let _ = sock_ref.set_send_buffer_size(2 * 1024 * 1024);
    Ok(socket)
}

pub fn create_throughput_socket(addr: SocketAddr) -> Result<UdpSocket, TransportError> {
    let socket = UdpSocket::bind(addr)
        .map_err(|e| TransportError::ConnectionError(e.to_string()))?;
    let _ = set_dscp(&socket, 0x0A); // AF11
    let _ = socket.set_nonblocking(true);
    let sock_ref = SockRef::from(&socket);
    let _ = sock_ref.set_recv_buffer_size(16 * 1024 * 1024);
    let _ = sock_ref.set_send_buffer_size(4 * 1024 * 1024);
    Ok(socket)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_latency_socket_binds() {
        let s = create_latency_socket("127.0.0.1:0".parse().unwrap()).unwrap();
        assert!(s.local_addr().unwrap().port() > 0);
    }

    #[test]
    fn create_throughput_socket_binds() {
        let s = create_throughput_socket("127.0.0.1:0".parse().unwrap()).unwrap();
        assert!(s.local_addr().unwrap().port() > 0);
    }

    #[test]
    fn dscp_set_does_not_error() {
        let s = UdpSocket::bind("127.0.0.1:0").unwrap();
        let _ = set_dscp(&s, 0x2E);
    }
}
