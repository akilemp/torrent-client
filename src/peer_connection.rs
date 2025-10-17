use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

use crate::handshake::{HANDSHAKE_LEN, Handshake, HandshakeError};

#[allow(dead_code)]
#[derive(Debug)]
pub struct PeerConnection {
    pub stream: TcpStream,
    pub peer_id: [u8; 20],
    pub reserved_flags: [u8; 8],
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
) -> Result<PeerConnection, HandshakeError> {
    let addr: SocketAddr = std::net::SocketAddr::V4(peer.socket_addr());

    println!("      Connecting to peer: {}", addr);

    let mut stream =
        TcpStream::connect_timeout(&addr, Duration::from_secs(3)).map_err(HandshakeError::Io)?;

    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .map_err(HandshakeError::Io)?;
    stream
        .set_write_timeout(Some(Duration::from_secs(5)))
        .map_err(HandshakeError::Io)?;

    let outgoing_handshake = Handshake::new(info_hash, client_peer_id);
    let handshake_bytes = outgoing_handshake.to_bytes();

    stream.write_all(&handshake_bytes)?;
    println!("      Successfully sent Handshake to {}", addr);

    let mut response_buffer = [0u8; HANDSHAKE_LEN];
    stream.read_exact(&mut response_buffer)?;
    println!("      Successfully received Handshake from {}", addr);

    let incoming_handshake = Handshake::from_bytes(response_buffer, info_hash)?;

    println!("      Validated received Handshake");
    Ok(PeerConnection {
        stream,
        peer_id: incoming_handshake.peer_id,
        reserved_flags: incoming_handshake.reserved,
    })
}
