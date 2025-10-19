use std::{
    fmt,
    io::{self, Cursor, Read, Write},
};

// The length of the protocol string "BitTorrent protocol"
const PSTR_LEN: u8 = 19;
// The protocol string itself
const PSTR: &[u8; PSTR_LEN as usize] = b"BitTorrent protocol";
// The fixed total length of the Handshake message
pub const HANDSHAKE_LEN: usize = 68;

/// Represents the 68-byte BitTorrent Handshake message.
#[derive(Debug, Clone)]
pub struct Handshake {
    pub pstrlen: u8,
    pub pstr: [u8; PSTR_LEN as usize],
    pub reserved: [u8; 8],
    pub info_hash: [u8; 20],
    pub peer_id: [u8; 20],
}

impl Handshake {
    pub fn new(info_hash: [u8; 20], client_peer_id: [u8; 20]) -> Self {
        Handshake {
            pstrlen: PSTR_LEN,
            pstr: *PSTR,
            reserved: [0; 8],
            info_hash: info_hash,
            peer_id: client_peer_id,
        }
    }

    pub fn to_bytes(&self) -> [u8; HANDSHAKE_LEN] {
        let mut buf = [0u8; HANDSHAKE_LEN];
        let mut cursor = Cursor::new(&mut buf[..]);

        cursor.write_all(&[self.pstrlen]).unwrap();
        cursor.write_all(&self.pstr).unwrap();
        cursor.write_all(&self.reserved).unwrap();
        cursor.write_all(&self.info_hash).unwrap();
        cursor.write_all(&self.peer_id).unwrap();

        buf
    }
}

// --- Handshake Deserialization and Validation ---

/// Custom error for Handshake processing.
#[derive(Debug)]
pub enum HandshakeError {
    Io(io::Error),
    InvalidLength,
    ProtocolMismatch,
    InfoHashMismatch,
}

// Implement Display for user-friendly error messages
impl fmt::Display for HandshakeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HandshakeError::Io(err) => write!(f, "I/O error during handshake: {}", err),
            HandshakeError::InvalidLength => write!(
                f,
                "Received handshake with incorrect length (must be 68 bytes)"
            ),
            HandshakeError::ProtocolMismatch => {
                write!(f, "Peer did not respond with 'BitTorrent protocol'")
            }
            HandshakeError::InfoHashMismatch => {
                write!(f, "Peer's info_hash does not match the torrent's info_hash")
            }
        }
    }
}

impl From<io::Error> for HandshakeError {
    fn from(err: io::Error) -> Self {
        HandshakeError::Io(err)
    }
}

impl Handshake {
    /// Deserializes a Handshake from a 68-byte buffer received from a peer.
    ///
    /// # Arguments
    /// * `buffer`: The 68-byte byte array read from the TCP stream.
    /// * `expected_info_hash`: The info hash your client is expecting to confirm the torrent ID.
    pub fn from_bytes(
        buffer: [u8; HANDSHAKE_LEN],
        expected_info_hash: [u8; 20],
    ) -> Result<Self, HandshakeError> {
        if buffer.len() != HANDSHAKE_LEN {
            return Err(HandshakeError::InvalidLength);
        }

        let mut cursor = Cursor::new(&buffer);

        let mut pstrlen_buf = [0; 1];
        cursor.read_exact(&mut pstrlen_buf)?;
        let pstrlen = pstrlen_buf[0];

        if pstrlen != PSTR_LEN {
            return Err(HandshakeError::ProtocolMismatch);
        }
        let mut pstr = [0; PSTR_LEN as usize];
        cursor.read_exact(&mut pstr)?;
        if &pstr != PSTR {
            return Err(HandshakeError::ProtocolMismatch);
        }

        let mut reserved = [0; 8];
        cursor.read_exact(&mut reserved)?;

        let mut info_hash = [0; 20];
        cursor.read_exact(&mut info_hash)?;

        if info_hash != expected_info_hash {
            return Err(HandshakeError::InfoHashMismatch);
        }

        let mut peer_id = [0; 20];
        cursor.read_exact(&mut peer_id)?;

        Ok(Handshake {
            pstrlen,
            pstr,
            reserved,
            info_hash,
            peer_id,
        })
    }
}
