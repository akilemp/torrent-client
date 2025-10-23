#![allow(dead_code)]

use std::net::SocketAddr;

use tokio::io::{AsyncRead, AsyncWrite, AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::{timeout, Duration};

use crate::bitfield::Bitfield;
use crate::handshake::{HANDSHAKE_LEN, Handshake, HandshakeError};
use crate::piece_proggress::PieceProgress;
use crate::torrent::VerifiedTorrent;
use crate::message::{Message, MessageId, MessageError};


#[derive(Debug)]
pub enum DownloadError {
    PieceNotAvailable,
    PeerChoked,
    InvalidHash,
    Io(std::io::Error),
    Protocol(String),
    Message(MessageError),
}


impl From<std::io::Error> for DownloadError {
    fn from(e: std::io::Error) -> Self {
        DownloadError::Io(e)
    }
}
impl From<MessageError> for DownloadError {
    fn from(e: MessageError) -> Self {
        DownloadError::Message(e)
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

        Ok(Message::from_bytes(&full_msg)?)
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
    ) -> Result<PeerConnection<TcpStream>, HandshakeError> {
        let addr: SocketAddr = std::net::SocketAddr::V4(peer.socket_addr());
        println!("      Connecting to peer: {}", addr);

        // Equivalent to connect_timeout
        let stream = timeout(Duration::from_secs(3), TcpStream::connect(addr))
        .await
        .map_err(|_| HandshakeError::Io(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            "connection timeout",
        )))??;

        let outgoing_handshake = Handshake::new(info_hash, client_peer_id);
        let out_bytes = outgoing_handshake.to_bytes();

        stream.writable().await?;
        stream.try_write(&out_bytes)?;

        let mut response_buffer = [0u8; HANDSHAKE_LEN];
        let mut owned_stream = stream;
        owned_stream.read_exact(&mut response_buffer).await?;

        let _incoming_handshake = Handshake::from_bytes(response_buffer, info_hash)?;

        Ok(PeerConnection {
            stream: owned_stream,
            is_choked: true,
            bitfield: None,
        })
    }

    pub async fn download_piece(
        &mut self,
        torrent: &VerifiedTorrent,
        peer_bitfield: &Bitfield, // TODO use self.bitfield
        piece_index: u32,
    ) -> Result<Vec<u8>, DownloadError> {
        if !peer_bitfield.has_piece(piece_index as usize) {
            return Err(DownloadError::PieceNotAvailable);
        }

        // Step 1. Send 'interested' and wait for unchoke
        self.write_message(&Message::interested()).await?;
        self.wait_for_unchoke().await?;

        // Step 2. Download piece blocks
        let piece_len = torrent.piece_length(piece_index as usize);
        let mut progress = PieceProgress::new(piece_index, piece_len);

        while !progress.is_complete() {
            for (offset, length) in progress.next_requests(PieceProgress::MAX_PIPELINE) {
                let req = Message::request(piece_index, offset, length);
                self.write_message(&req).await?;
            }

            let msg = self.read_message().await?;
            if let Some(MessageId::Piece) = msg.id {
                let (index, begin, block) = Message::parse_piece_payload(&msg.payload)?;
                if index == piece_index {
                    progress.mark_block(begin as usize, &block);
                }
            }
        }

        // Step 3. Verify hash
        let expecter_hash = torrent.piece_hashes[piece_index as usize];
        if !progress.verify(&expecter_hash) {
            return Err(DownloadError::InvalidHash);
        }

        // Step 4. Notify peer we have the piece
        let _ = self.write_message(&Message::have(piece_index)).await?;
        Ok(progress.data)
    }

    async fn wait_for_unchoke(&mut self) -> Result<(), DownloadError> {
        loop {
            let msg = self.read_message().await?;
            match msg.id {
                Some(MessageId::Unchoke) => return Ok(()),
                Some(MessageId::Choke) => return Err(DownloadError::PeerChoked),
                _ => continue,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{self, AsyncReadExt, AsyncWriteExt, duplex, DuplexStream};
    use crate::{
        message::{Message, MessageId},
        bitfield::Bitfield,
        piece_proggress::PieceProgress,
        torrent::VerifiedTorrent,
    };
    use sha1::{Digest, Sha1};

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
            let unchoke = Message{id: Some(MessageId::Unchoke), payload: vec![]}.to_bytes();
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
            let piece_msg = Message{id: Some(MessageId::Piece), payload: payload}.to_bytes();
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

        // Call download_piece()
        let result = conn.download_piece(&torrent, &bitfield, 0).await;

        // Verify results
        assert!(result.is_ok(), "download_piece failed: {:?}", result);
        let data = result.unwrap();
        assert_eq!(data, piece_data);
    }
}
