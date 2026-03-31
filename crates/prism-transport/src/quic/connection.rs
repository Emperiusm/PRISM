// QUIC connection lifecycle management.

use bytes::Bytes;
use async_trait::async_trait;
use tokio::sync::broadcast;
use crate::connection::*;

pub struct QuicConnection {
    connection: quinn::Connection,
    event_tx: broadcast::Sender<TransportEvent>,
}

impl QuicConnection {
    pub fn new(connection: quinn::Connection) -> Self {
        let (event_tx, _) = broadcast::channel(64);
        Self { connection, event_tx }
    }
}

#[async_trait]
impl PrismConnection for QuicConnection {
    fn try_send_datagram(&self, data: Bytes) -> Result<(), TransportError> {
        let max = self.connection.max_datagram_size().unwrap_or(0);
        let size = data.len();
        self.connection.send_datagram(data).map_err(|e| {
            use quinn::SendDatagramError::*;
            match e {
                UnsupportedByPeer | Disabled => TransportError::DatagramUnsupported,
                TooLarge => TransportError::DatagramTooLarge { size, max },
                ConnectionLost(_) => TransportError::ConnectionClosed,
            }
        })
    }

    async fn send_datagram(&self, data: Bytes) -> Result<(), TransportError> {
        if let Some(max) = self.connection.max_datagram_size() {
            if data.len() <= max {
                return self.try_send_datagram(data);
            }
        }
        // Spill to uni stream when datagram is too large
        let mut s = self.connection
            .open_uni()
            .await
            .map_err(|e| TransportError::StreamError(e.to_string()))?;
        s.write_all(&data)
            .await
            .map_err(|e| TransportError::StreamError(e.to_string()))?;
        s.finish().map_err(|e| TransportError::StreamError(e.to_string()))?;
        Ok(())
    }

    async fn recv_datagram(&self) -> Result<Bytes, TransportError> {
        self.connection
            .read_datagram()
            .await
            .map_err(|e| TransportError::ConnectionError(e.to_string()))
    }

    async fn open_bi(&self, priority: StreamPriority) -> Result<(OwnedSendStream, OwnedRecvStream), TransportError> {
        let (send, recv) = self.connection
            .open_bi()
            .await
            .map_err(|e| TransportError::StreamError(e.to_string()))?;
        let _ = send.set_priority(priority.to_quinn_priority());
        Ok((OwnedSendStream::from_quic(send), OwnedRecvStream::from_quic(recv)))
    }

    async fn open_uni(&self, priority: StreamPriority) -> Result<OwnedSendStream, TransportError> {
        let send = self.connection
            .open_uni()
            .await
            .map_err(|e| TransportError::StreamError(e.to_string()))?;
        let _ = send.set_priority(priority.to_quinn_priority());
        Ok(OwnedSendStream::from_quic(send))
    }

    async fn accept_bi(&self) -> Result<(OwnedSendStream, OwnedRecvStream), TransportError> {
        let (send, recv) = self.connection
            .accept_bi()
            .await
            .map_err(|e| TransportError::ConnectionError(e.to_string()))?;
        Ok((OwnedSendStream::from_quic(send), OwnedRecvStream::from_quic(recv)))
    }

    async fn accept_uni(&self) -> Result<OwnedRecvStream, TransportError> {
        let recv = self.connection
            .accept_uni()
            .await
            .map_err(|e| TransportError::ConnectionError(e.to_string()))?;
        Ok(OwnedRecvStream::from_quic(recv))
    }

    fn metrics(&self) -> TransportMetrics {
        let stats = self.connection.stats();
        TransportMetrics {
            rtt_us: stats.path.rtt.as_micros() as u64,
            bytes_sent: stats.udp_tx.bytes,
            bytes_received: stats.udp_rx.bytes,
            datagrams_sent: stats.udp_tx.datagrams,
            transport_type: TransportType::Quic,
            ..TransportMetrics::default()
        }
    }

    fn transport_type(&self) -> TransportType {
        TransportType::Quic
    }

    fn max_datagram_size(&self) -> usize {
        self.connection.max_datagram_size().unwrap_or(0)
    }

    fn events(&self) -> broadcast::Receiver<TransportEvent> {
        self.event_tx.subscribe()
    }

    async fn close(&self) {
        self.connection.close(0u32.into(), b"close");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    async fn loopback_pair() -> (quinn::Connection, quinn::Connection) {
        let rcgen::CertifiedKey { cert, key_pair } =
            rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
        let cert_der = cert.der().clone();
        let key_der = rustls::pki_types::PrivateKeyDer::Pkcs8(
            rustls::pki_types::PrivatePkcs8KeyDer::from(key_pair.serialize_der()),
        );

        let mut server_config =
            quinn::ServerConfig::with_single_cert(vec![cert_der.clone()], key_der).unwrap();
        server_config.transport_config(Arc::new(super::super::config::latency_transport_config(None)));

        let server_endpoint =
            quinn::Endpoint::server(server_config, "127.0.0.1:0".parse().unwrap()).unwrap();
        let server_addr = server_endpoint.local_addr().unwrap();

        let mut roots = rustls::RootCertStore::empty();
        roots.add(cert_der).unwrap();
        let client_crypto = rustls::ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();
        let client_config = quinn::ClientConfig::new(Arc::new(
            quinn::crypto::rustls::QuicClientConfig::try_from(client_crypto).unwrap(),
        ));

        let mut client_endpoint =
            quinn::Endpoint::client("0.0.0.0:0".parse().unwrap()).unwrap();
        client_endpoint.set_default_client_config(client_config);

        let connect_future = client_endpoint.connect(server_addr, "localhost").unwrap();
        let accept_future = server_endpoint.accept().await.unwrap();

        let (client_conn, server_conn) = tokio::join!(
            async { connect_future.await.unwrap() },
            async { accept_future.await.unwrap() },
        );
        (client_conn, server_conn)
    }

    #[tokio::test]
    async fn quic_connection_metadata() {
        let (c, _s) = loopback_pair().await;
        let qc = QuicConnection::new(c);
        assert_eq!(qc.transport_type(), TransportType::Quic);
        assert!(qc.max_datagram_size() > 0);
    }

    #[tokio::test]
    async fn quic_datagram_roundtrip() {
        let (c, s) = loopback_pair().await;
        let cqc = QuicConnection::new(c);
        let sqc = QuicConnection::new(s);
        cqc.try_send_datagram(Bytes::from_static(b"hello")).unwrap();
        let received = sqc.recv_datagram().await.unwrap();
        assert_eq!(received, Bytes::from_static(b"hello"));
    }

    #[tokio::test]
    async fn quic_stream_roundtrip() {
        let (c, s) = loopback_pair().await;
        let cqc = QuicConnection::new(c);
        let sqc = QuicConnection::new(s);
        let (mut send, _recv) = cqc.open_bi(StreamPriority::Normal).await.unwrap();
        send.write(b"stream data").await.unwrap();
        send.finish().await.unwrap();
        let (_send, recv) = sqc.accept_bi().await.unwrap();
        let data = recv.read_to_end(1024).await.unwrap();
        assert_eq!(data, b"stream data");
    }

    #[tokio::test]
    async fn quic_uni_stream() {
        let (c, s) = loopback_pair().await;
        let cqc = QuicConnection::new(c);
        let sqc = QuicConnection::new(s);
        let mut send = cqc.open_uni(StreamPriority::High).await.unwrap();
        send.write(b"unidirectional").await.unwrap();
        send.finish().await.unwrap();
        let recv = sqc.accept_uni().await.unwrap();
        let data = recv.read_to_end(1024).await.unwrap();
        assert_eq!(data, b"unidirectional");
    }

    #[tokio::test]
    async fn quic_close() {
        let (c, _s) = loopback_pair().await;
        let qc = QuicConnection::new(c);
        qc.close().await;
        let result = qc.try_send_datagram(Bytes::from_static(b"after close"));
        assert!(result.is_err());
    }
}
