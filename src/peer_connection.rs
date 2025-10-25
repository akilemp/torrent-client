#![allow(dead_code)]

use std::error::Error;
use std::fmt;
use std::net::SocketAddr;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::error::Elapsed;
use tokio::time::{Duration, timeout};

use crate::bitfield::Bitfield;
use crate::handshake::{HANDSHAKE_LEN, Handshake, HandshakeError};
use crate::message::{Message, MessageError, MessageId};
use crate::piece_proggress::PieceProgress;
use crate::torrent::VerifiedTorrent;

#[derive(Debug)]
pub enum ConnectionError {
    PieceNotAvailable,
    PeerChoked,
    InvalidHash,
    Io(std::io::Error),
    Protocol(String),
    Message(MessageError),
    Handshake(HandshakeError),
}

impl fmt::Display for ConnectionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConnectionError::PieceNotAvailable => write!(f, "Piece not available from peer"),
            ConnectionError::PeerChoked => write!(f, "Peer is choked"),
            ConnectionError::InvalidHash => write!(f, "Invalid piece hash"),
            ConnectionError::Io(e) => write!(f, "I/O error: {}", e),
            ConnectionError::Protocol(msg) => write!(f, "Protocol error: {}", msg),
            ConnectionError::Message(e) => write!(f, "Message error: {}", e),
            ConnectionError::Handshake(e) => write!(f, "Handshake error: {}", e),
        }
    }
}

// --- Implement std::error::Error for integration with anyhow, ? etc. ---
impl Error for ConnectionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            ConnectionError::Io(e) => Some(e),
            ConnectionError::Message(e) => Some(e),
            ConnectionError::Handshake(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for ConnectionError {
    fn from(e: std::io::Error) -> Self {
        ConnectionError::Io(e)
    }
}
impl From<MessageError> for ConnectionError {
    fn from(e: MessageError) -> Self {
        ConnectionError::Message(e)
    }
}
impl From<HandshakeError> for ConnectionError {
    fn from(e: HandshakeError) -> Self {
        ConnectionError::Handshake(e)
    }
}
impl From<Elapsed> for ConnectionError {
    fn from(_e: Elapsed) -> Self {
        ConnectionError::Protocol("Timeout".into())
    }
}

#[derive(Debug)]
pub struct PeerConnection<S: AsyncRead + AsyncWrite + Unpin> {
    pub stream: S,
    pub is_choked: bool,
    pub bitfield: Option<Bitfield>,
}

impl<S: AsyncRead + AsyncWrite + Unpin> PeerConnection<S> {
    pub fn new(stream: S) -> Self {
        Self {
            stream,
            is_choked: true,
            bitfield: None,
        }
    }

    pub async fn read_message(&mut self) -> Result<Message, MessageError> {
        let mut len_buf = [0u8; 4];
        self.stream.read_exact(&mut len_buf).await?;
        let msg_len = u32::from_be_bytes(len_buf) as usize;

        if msg_len == 0 {
            return Ok(Message::keep_alive());
        }

        let mut msg_buf = vec![0u8; msg_len];
        self.stream.read_exact(&mut msg_buf).await?;

        let mut full_msg = len_buf.to_vec();
        full_msg.extend_from_slice(&msg_buf);

        Message::from_bytes(&full_msg)
    }

    pub async fn write_message(&mut self, msg: &Message) -> Result<(), MessageError> {
        let bytes = msg.to_bytes();
        self.stream.write_all(&bytes).await?;
        Ok(())
    }

    /// Establishes an async TCP connection, performs handshake, validates response.
    pub async fn connect(
        peer: &crate::peer::Peer,
        info_hash: [u8; 20],
        client_peer_id: [u8; 20],
    ) -> Result<PeerConnection<TcpStream>, ConnectionError> {
        let addr: SocketAddr = std::net::SocketAddr::V4(peer.socket_addr());
        println!("      Connecting to peer: {}", addr);

        // Equivalent to connect_timeout
        let stream = timeout(Duration::from_secs(3), TcpStream::connect(addr))
            .await
            .map_err(|_| {
                ConnectionError::Io(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "connection timeout",
                ))
            })??;

        let (stream, _incoming_handshake) =
            Self::perform_handshake(stream, info_hash, client_peer_id).await?;

        let mut conn = PeerConnection::new(stream);

        conn.write_message(&Message::interested()).await?;
        conn.write_message(&Message {
            id: Some(MessageId::Unchoke),
            payload: vec![],
        })
        .await?;

        Ok(conn)
    }

    /// Perform the BitTorrent handshake with the given peer.
    ///
    /// Sends our handshake, reads the peer's handshake, and validates it.
    async fn perform_handshake(
        mut stream: TcpStream,
        info_hash: [u8; 20],
        client_peer_id: [u8; 20],
    ) -> Result<(TcpStream, Handshake), HandshakeError> {
        let outgoing_handshake = Handshake::new(info_hash, client_peer_id);
        let out_bytes = outgoing_handshake.to_bytes();

        // Ensure stream is ready for writing
        stream.writable().await?;
        stream.write_all(&out_bytes).await?;

        // Read peer's handshake
        let mut response_buffer = [0u8; HANDSHAKE_LEN];
        stream.read_exact(&mut response_buffer).await?;

        // Parse and validate
        let incoming_handshake = Handshake::from_bytes(response_buffer, info_hash)?;

        Ok((stream, incoming_handshake))
    }

    /// Waits until a Bitfield message is received, ignoring other messages.
    /// Returns an error if the connection ends or an unexpected condition occurs.
    pub async fn wait_for_bitfield(&mut self) -> Result<Bitfield, ConnectionError> {
        match timeout(Duration::from_secs(5), self.wait_for_bitfield_inner()).await? {
            Ok(result) => Ok(result),
            Err(_) => Err(ConnectionError::Protocol(
                "Timed out waiting for Bitfield".into(),
            )),
        }
    }

    async fn wait_for_bitfield_inner(&mut self) -> Result<Bitfield, ConnectionError> {
        loop {
            let msg = self.read_message().await?;

            match msg.id {
                Some(MessageId::Bitfield) => {
                    return Ok(Bitfield::from_bytes(msg.payload));
                }
                Some(MessageId::Unchoke) => {
                    self.is_choked = false;
                    continue;
                }
                Some(MessageId::Choke) => {
                    self.is_choked = true;
                    continue;
                }
                Some(other) => {
                    eprintln!("Ignoring message {:?} while waiting for Bitfield", other);
                    continue;
                }
                None => {
                    eprintln!("Peer sent keep-alive while waiting for Bitfield");
                    continue;
                }
            }
        }
    }

    pub async fn download_piece(
        &mut self,
        torrent: &VerifiedTorrent,
        piece_index: u32,
    ) -> Result<Vec<u8>, ConnectionError> {
        let bitfield = match &self.bitfield {
            Some(bf) => bf,
            None => return Err(ConnectionError::Protocol("No bitfield".into())),
        };

        if !bitfield.has_piece(piece_index as usize) {
            return Err(ConnectionError::PieceNotAvailable);
        }

        // Step 2. Download piece blocks
        let piece_len = torrent.piece_length(piece_index as usize);
        let mut progress = PieceProgress::new(piece_index, piece_len);

        while !progress.is_complete() {
            if self.is_choked {
                self.wait_for_unchoke().await?;
            }

            for (offset, length) in progress.next_requests(PieceProgress::MAX_PIPELINE) {
                let req = Message::request(piece_index, offset, length);
                self.write_message(&req).await?;
            }

            let msg = self.read_message().await?;
            match msg.id {
                Some(MessageId::Piece) => {
                    let (index, begin, block) = Message::parse_piece_payload(&msg.payload)?;
                    if index == piece_index {
                        progress.mark_block(begin as usize, &block);
                    }
                }
                Some(MessageId::Choke) => {
                    self.is_choked = true;
                }
                Some(MessageId::Unchoke) => {
                    self.is_choked = false;
                }
                Some(_other) => return Err(ConnectionError::Protocol("Didnt get a piece".into())),
                None => continue,
            }
        }

        // Step 3. Verify hash
        let expected_hash = torrent.piece_hashes[piece_index as usize];
        if !progress.verify(&expected_hash) {
            return Err(ConnectionError::InvalidHash);
        }

        // Step 4. Notify peer we have the piece
        self.write_message(&Message::have(piece_index)).await?;
        Ok(progress.data)
    }

    async fn wait_for_unchoke(&mut self) -> Result<(), ConnectionError> {
        loop {
            let msg = self.read_message().await?;
            match msg.id {
                Some(MessageId::Unchoke) => {
                    self.is_choked = false;
                    return Ok(());
                }
                _ => continue,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        bitfield::Bitfield,
        message::{Message, MessageId},
        piece_proggress::PieceProgress,
        torrent::VerifiedTorrent,
    };
    use sha1::{Digest, Sha1};
    use tokio::io::{self, AsyncReadExt, AsyncWriteExt, DuplexStream, duplex};

    fn make_bitfield_message() -> Vec<u8> {
        let payload = vec![0b_1010_1010, 0b_1100_0000];
        let msg = Message {
            id: Some(MessageId::Bitfield),
            payload,
        };
        msg.to_bytes()
    }

    #[tokio::test]
    async fn test_read_message_from_stream() {
        let data = make_bitfield_message();
        let (client, mut server): (DuplexStream, DuplexStream) = io::duplex(64);

        // preload message into server
        tokio::spawn(async move {
            server.write_all(&data).await.unwrap();
        });

        let mut conn = PeerConnection {
            stream: client,
            is_choked: true,
            bitfield: None,
        };

        let msg = conn.read_message().await.expect("failed to read message");
        assert_eq!(msg.id, Some(MessageId::Bitfield));
        assert_eq!(msg.payload, vec![0b_1010_1010, 0b_1100_0000]);
    }

    #[tokio::test]
    async fn test_write_and_read_message() {
        use tokio::io::{self, AsyncReadExt, AsyncWriteExt, DuplexStream};
        let payload = vec![0x11, 0x22, 0x33];
        let message = Message {
            id: Some(MessageId::Bitfield),
            payload: payload.clone(),
        };

        let (a, mut b): (DuplexStream, DuplexStream) = io::duplex(64);

        let mut conn = PeerConnection {
            stream: a,
            is_choked: true,
            bitfield: None,
        };

        conn.write_message(&message).await.expect("write failed");

        // ✅ Close write half so reader sees EOF
        conn.stream.shutdown().await.unwrap();

        // Read all written data
        let mut buf = Vec::new();
        b.read_to_end(&mut buf).await.unwrap();

        let msg = Message::from_bytes(&buf).unwrap();

        assert_eq!(msg.id, Some(MessageId::Bitfield));
        assert_eq!(msg.payload, payload);
    }

    #[tokio::test]
    async fn test_write_to_stream_keep_alive() {
        use tokio::io::{self, AsyncReadExt, AsyncWriteExt, DuplexStream};
        let msg = Message::keep_alive();
        let (a, mut b): (DuplexStream, DuplexStream) = io::duplex(64);

        let mut conn = PeerConnection {
            stream: a,
            is_choked: true,
            bitfield: None,
        };

        conn.write_message(&msg).await.unwrap();

        // ✅ Close to signal EOF
        conn.stream.shutdown().await.unwrap();

        let mut buf = Vec::new();
        b.read_to_end(&mut buf).await.unwrap();

        assert_eq!(buf, vec![0, 0, 0, 0]);
    }

    #[tokio::test]
    async fn test_download_piece_successful() {
        // Create a duplex stream (like a virtual TCP connection)
        let (client_side, mut server_side) = duplex(1024);

        // ========== Simulated Peer Behavior (Server Side Task) ==========
        tokio::spawn(async move {
            // 1️⃣ Respond with an UNCHOKE
            let unchoke = Message {
                id: Some(MessageId::Unchoke),
                payload: vec![],
            }
            .to_bytes();
            server_side.write_all(&unchoke).await.unwrap();

            // 2️⃣ Wait for INTERESTED and REQUESTs
            let mut buf = [0u8; 1024];
            let _n = server_side.read(&mut buf).await.unwrap();
            // (We’re not parsing here, just consuming bytes)

            // 3️⃣ Send back a PIECE message
            let piece_index: u32 = 0;
            let begin: u32 = 0;
            let block_data = vec![1u8; PieceProgress::BLOCK_SIZE];
            let mut payload = Vec::new();
            payload.extend_from_slice(&piece_index.to_be_bytes());
            payload.extend_from_slice(&begin.to_be_bytes());
            payload.extend_from_slice(&block_data);
            let piece_msg = Message {
                id: Some(MessageId::Piece),
                payload: payload,
            }
            .to_bytes();
            server_side.write_all(&piece_msg).await.unwrap();

            tokio::time::sleep(Duration::from_millis(10)).await;
        });

        // ========== Client (Downloader) Side ==========
        // Hash of the expected piece
        let piece_data = vec![1u8; PieceProgress::BLOCK_SIZE];
        let hash = Sha1::digest(&piece_data);
        let mut hash_arr = [0u8; 20];
        hash_arr.copy_from_slice(&hash[..]);

        let torrent = VerifiedTorrent {
            announce: "http://tracker.local".to_string(),
            info_hash: [0; 20],
            name: "test".into(),
            piece_length: PieceProgress::BLOCK_SIZE as u64,
            piece_hashes: vec![hash_arr],
            total_size: PieceProgress::BLOCK_SIZE as u64,
        };

        let mut bitfield = Bitfield::new(1);
        bitfield.set_piece(0);

        // Create a PeerConnection over the client side
        let mut conn = PeerConnection::new(client_side);
        conn.bitfield = Some(bitfield);

        // Call download_piece()
        let result = conn.download_piece(&torrent, 0).await;

        // Verify results
        assert!(result.is_ok(), "download_piece failed: {:?}", result);
        let data = result.unwrap();
        assert_eq!(data, piece_data);
    }
}
