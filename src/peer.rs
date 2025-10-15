use rand::Rng;
use std::net::{Ipv4Addr, SocketAddrV4};

#[derive(Debug, Clone)]
pub struct _Peer {
    pub ip: Ipv4Addr,
    pub port: u16,
}

impl _Peer {
    pub fn _socket_addr(&self) -> SocketAddrV4 {
        SocketAddrV4::new(self.ip, self.port)
    }
}

pub fn generate_peer_id() -> [u8; 20] {
    let mut rng = rand::rng();
    let mut peer_id = [0u8; 20];

    rng.fill(&mut peer_id[..]);

    peer_id
}
