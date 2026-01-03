//! Simple SRT listener for CLI use
//!
//! Accepts a single incoming SRT connection and feeds data to the demuxer.
//! Unlike the full ingest-srt service, this doesn't require session management,
//! JWT authentication, or multi-connection routing.

use futures::StreamExt;
use srt_tokio::SrtListener;
use tokio::sync::mpsc;

/// Simple SRT listener that accepts a single connection
pub struct SimpleSrtListener {
    port: u16,
}

impl SimpleSrtListener {
    pub fn new(port: u16) -> Self {
        Self { port }
    }

    /// Listen for a single SRT connection and return the data as a stream
    /// 
    /// This is designed for CLI use where we expect one connection at a time.
    pub async fn accept_single(&self) -> Result<SrtConnection, SrtListenerError> {
        let bind_addr = format!("0.0.0.0:{}", self.port);
        tracing::info!("Waiting for SRT connection on {}...", bind_addr);

        let socket_addr: std::net::SocketAddr = bind_addr
            .parse()
            .map_err(|e| SrtListenerError::Bind(format!("Invalid address: {}", e)))?;

        let (_listener, mut srt_incoming) = SrtListener::builder()
            .bind(socket_addr)
            .await
            .map_err(|e| SrtListenerError::Bind(e.to_string()))?;

        tracing::info!(
            "SRT listener ready. Push to: srt://localhost:{}",
            self.port
        );
        tracing::info!("Example: ffmpeg -re -i input.mp4 -c copy -f mpegts srt://localhost:{}", self.port);

        // Wait for first connection
        let incoming = srt_incoming.incoming();
        let request = incoming.next().await
            .ok_or_else(|| SrtListenerError::Connection("Listener closed before connection".into()))?;

        let streamid = request.stream_id().map(|s| s.to_string());
        tracing::info!("Incoming SRT connection (streamid: {:?})", streamid);

        // Accept the connection
        let socket = request
            .accept(None)
            .await
            .map_err(|e| SrtListenerError::Connection(e.to_string()))?;

        tracing::info!("SRT connection accepted, starting to receive data...");

        Ok(SrtConnection {
            socket,
            streamid,
        })
    }
}

/// Active SRT connection
pub struct SrtConnection {
    socket: srt_tokio::SrtSocket,
    pub streamid: Option<String>,
}

impl SrtConnection {
    /// Create a channel to receive MPEG-TS data from this connection
    ///
    /// This spawns a task that reads from the SRT socket and sends data
    /// to the returned receiver.
    pub fn into_data_receiver(mut self) -> mpsc::Receiver<Vec<u8>> {
        let (tx, rx) = mpsc::channel::<Vec<u8>>(64);

        tokio::spawn(async move {
            let mut packets_received: u64 = 0;
            let mut bytes_received: u64 = 0;

            while let Some(result) = self.socket.next().await {
                match result {
                    Ok((_, data)) => {
                        packets_received += 1;
                        bytes_received += data.len() as u64;

                        if packets_received == 1 {
                            tracing::info!("Receiving SRT data...");
                        }

                        if tx.send(data.to_vec()).await.is_err() {
                            tracing::debug!("Receiver dropped, stopping SRT read loop");
                            break;
                        }
                    }
                    Err(e) => {
                        tracing::warn!("SRT receive error: {}", e);
                        break;
                    }
                }
            }

            tracing::info!(
                "SRT stream ended: {} packets, {} bytes received",
                packets_received,
                bytes_received
            );
        });

        rx
    }
}

/// SRT listener errors
#[derive(Debug, thiserror::Error)]
pub enum SrtListenerError {
    #[error("Failed to bind: {0}")]
    Bind(String),

    #[error("Connection error: {0}")]
    Connection(String),

    #[error("Receive error: {0}")]
    Receive(String),
}
