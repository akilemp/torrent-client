use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

use crate::handshake::{HANDSHAKE_LEN, Handshake, HandshakeError};
use crate::message::{Message, MessageError};

#[allow(dead_code)]
#[derive(Debug)]
pub struct PeerConnection<S: Read + Write> {
    stream: S,
    pub peer_id: [u8; 20],
    pub is_choked: bool,
}

impl<S: Read + Write> PeerConnection<S> {
    pub fn _new(stream: S, peer_id: [u8; 20]) -> Self {
        Self {
            stream,
            peer_id,
            is_choked: true,
        }
    }

    pub fn read_message(&mut self) -> Result<Message, MessageError> {
        Message::read_from_stream(&mut self.stream)
    }

    pub fn write_message(&mut self, msg: &Message) -> Result<(), MessageError> {
        msg.write_to_stream(&mut self.stream)
    }
}

/// Establishes a TCP connection, performs the Handshake, and validates the response.
///
/// # Arguments
/// * `peer`: The address (IP and Port) of the target peer.
/// * `info_hash`: The 20-byte hash of the torrent being downloaded.
/// * `client_peer_id`: Your client's 20-byte ID.
pub fn perform_handshake(
    peer: &crate::peer::Peer,
    info_hash: [u8; 20],
    client_peer_id: [u8; 20],
) -> Result<PeerConnection<TcpStream>, HandshakeError> {
    let addr: SocketAddr = std::net::SocketAddr::V4(peer.socket_addr());

    println!("      Connecting to peer: {}", addr);

    let mut stream =
        TcpStream::connect_timeout(&addr, Duration::from_secs(3)).map_err(HandshakeError::Io)?;

    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    stream.set_write_timeout(Some(Duration::from_secs(5)))?;

    let outgoing_handshake = Handshake::new(info_hash, client_peer_id);
    let out_handshake_bytes = outgoing_handshake.to_bytes();

    stream.write_all(&out_handshake_bytes)?;
    println!("      Successfully sent Handshake to {}", addr);

    let mut response_buffer = [0u8; HANDSHAKE_LEN];
    stream.read_exact(&mut response_buffer)?;
    println!("      Successfully received Handshake from {}", addr);

    let incoming_handshake = Handshake::from_bytes(response_buffer, info_hash)?;

    println!("      Validated received Handshake");
    Ok(PeerConnection {
        stream,
        peer_id: incoming_handshake.peer_id,
        is_choked: true,
    })
}

#[cfg(test)]
mod tests {
    use crate::message::MessageId;

    use super::*;
    use std::io::Cursor;

    fn make_bitfield_message() -> Vec<u8> {
        let payload = vec![0b_1010_1010, 0b_1100_0000];
        let msg = Message {
            id: Some(MessageId::Bitfield),
            payload: payload,
        };
        msg.to_bytes()
    }

    #[test]
    fn test_read_message_from_cursor() {
        let data = make_bitfield_message();
        let cursor = Cursor::new(data);

        let mut conn = PeerConnection {
            stream: cursor,
            peer_id: [0; 20],
            is_choked: true,
        };

        let msg = conn.read_message().expect("failed to read message");
        assert_eq!(msg.id, Some(MessageId::Bitfield));
        assert_eq!(msg.payload, vec![0b_1010_1010, 0b_1100_0000]);
    }

    #[test]
    fn test_write_and_read_message() {
        let payload = vec![0x11, 0x22, 0x33];
        let message = Message {
            id: Some(MessageId::Bitfield),
            payload: payload.clone(),
        };

        let mut buffer = Cursor::new(Vec::new());

        message.write_to_stream(&mut buffer).expect("write failed");

        // Reset cursor to read from beginning
        buffer.set_position(0);

        // Use PeerConnection with buffer
        let mut conn = PeerConnection {
            stream: buffer,
            peer_id: [1; 20],
            is_choked: true,
        };

        let read_msg = conn.read_message().expect("read failed");
        assert_eq!(read_msg.id, Some(MessageId::Bitfield));
        assert_eq!(read_msg.payload, payload);
    }
}
