use rand::Rng;
use std::io::{self};
use std::net::{Ipv4Addr, SocketAddrV4};

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Peer {
    pub ip: Ipv4Addr,
    pub port: u16,
}

impl Peer {
    pub fn socket_addr(&self) -> SocketAddrV4 {
        SocketAddrV4::new(self.ip, self.port)
    }
}

pub fn generate_peer_id() -> [u8; 20] {
    let mut rng = rand::rng();
    let mut peer_id = [0u8; 20];

    rng.fill(&mut peer_id[..]);

    peer_id
}

/// Parses the compact 6-byte peer list string.
pub fn parse_compact_peers(bytes: &[u8]) -> Result<Vec<Peer>, io::Error> {
    if bytes.len() % 6 != 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Compact peer list length is not a multiple of 6",
        ));
    }

    let mut peers = Vec::new();
    let mut chunks = bytes.chunks_exact(6);

    while let Some(chunk) = chunks.next() {
        // Bytes 0-3 are the IP address
        let ip_bytes: [u8; 4] = chunk[0..4].try_into().unwrap();
        let ip = Ipv4Addr::from(ip_bytes);

        // Bytes 4-5 are the port (big-endian)
        let mut port_bytes: [u8; 2] = [0; 2];
        port_bytes.copy_from_slice(&chunk[4..6]);
        // Convert big-endian bytes to a u16
        let port = u16::from_be_bytes(port_bytes);

        peers.push(Peer { ip: ip, port: port });
    }

    Ok(peers)
}
