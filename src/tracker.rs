use std::error::Error;
use std::fmt;

use reqwest::Client;
use serde::Deserialize;

use crate::peer::{self, Peer};
use crate::torrent::VerifiedTorrent;

#[derive(Debug)]
pub enum TrackerError {
    HttpClient(reqwest::Error),
    BencodeDecode(serde_bencode::Error),
    PeerParse(std::io::Error),
}

impl fmt::Display for TrackerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TrackerError::HttpClient(e) => {
                write!(f, "Http client error while contacting tracker: {}", e)
            }
            TrackerError::BencodeDecode(e) => write!(f, "Failed to decode tracker response: {}", e),
            TrackerError::PeerParse(e) => write!(f, "Failed to parse peer list: {}", e),
        }
    }
}

impl Error for TrackerError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            TrackerError::HttpClient(e) => Some(e),
            TrackerError::BencodeDecode(e) => Some(e),
            TrackerError::PeerParse(e) => Some(e),
        }
    }
}

impl From<reqwest::Error> for TrackerError {
    fn from(e: reqwest::Error) -> Self {
        TrackerError::HttpClient(e)
    }
}

impl From<serde_bencode::Error> for TrackerError {
    fn from(e: serde_bencode::Error) -> Self {
        TrackerError::BencodeDecode(e)
    }
}

impl From<std::io::Error> for TrackerError {
    fn from(e: std::io::Error) -> Self {
        TrackerError::PeerParse(e)
    }
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct TrackerResponse {
    /// The interval in seconds to wait between requests.
    pub interval: i64,

    /// The number of seeders. Optional.
    #[serde(default)]
    pub complete: Option<i64>,

    /// The number of leechers. Optional.
    #[serde(default)]
    pub incomplete: Option<i64>,

    /// The peers in the "compact" format (required by your URL: compact=1).
    #[serde(with = "serde_bytes")]
    pub peers: Vec<u8>,

    /// Optional IPv6 peers (18 bytes per peer)
    #[serde(default)]
    #[serde(with = "serde_bytes")]
    pub peers6: Option<Vec<u8>>,
}

fn percent_encode_bytes(data: &[u8]) -> String {
    let mut encoded = String::with_capacity(data.len() * 3);
    for byte in data {
        encoded.push('%');
        encoded.push_str(&format!("{:02X}", byte));
    }
    encoded
}

pub fn build_tracker_url(torrent: &VerifiedTorrent, peer_id: &[u8; 20]) -> String {
    let info_hash_encoded = percent_encode_bytes(&torrent.info_hash);
    let peer_id_encoded = percent_encode_bytes(peer_id);

    format!(
        "{announce}?info_hash={info_hash_enc}&peer_id={peer_id_enc}&port={port}&uploaded={uploaded}&downloaded={downloaded}&left={left}&compact={compact}&event={event}",
        announce = torrent.announce,
        info_hash_enc = info_hash_encoded,
        peer_id_enc = peer_id_encoded,
        port = "6881",
        uploaded = "0",
        downloaded = "0",
        left = torrent.total_size,
        compact = "1",
        event = "started",
    )
}

/// Asynchronously fetch peers from the tracker and return a list of `Peer`s.
pub async fn get_peers(
    torrent: &VerifiedTorrent,
    peer_id: &[u8; 20],
) -> Result<Vec<Peer>, TrackerError> {
    let url = build_tracker_url(torrent, peer_id);

    let client = Client::new();
    let response = client.get(&url).send().await?; // <-- await here

    if !response.status().is_success() {
        return Err(TrackerError::HttpClient(
            response.error_for_status().unwrap_err(),
        ));
    }

    let response_bytes = response.bytes().await?; // <-- await here
    let tracker_response: TrackerResponse = serde_bencode::from_bytes(&response_bytes)?;

    let peers = peer::parse_compact_peers(&tracker_response.peers)?;

    Ok(peers)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_tracker_url_encoding() {
        let info_hash_hex = "2ced861966e919e5ca9e35d27dc23e0b02fb7ff8";
        let peer_id_hex = "b1d07b04d15ad13b22b359757dbc6a563e89b296";

        let total_size = 821035008;
        let announce_url = "http://bttracker.debian.org:6969/announce".to_string();

        let info_hash_bytes: [u8; 20] = hex::decode(info_hash_hex)
            .unwrap()
            .try_into()
            .expect("Hex hash must be 20 bytes long");

        let peer_id_bytes: [u8; 20] = hex::decode(peer_id_hex)
            .unwrap()
            .try_into()
            .expect("Hex peer ID must be 20 bytes long");

        let mock_torrent = crate::torrent::VerifiedTorrent {
            announce: announce_url,
            info_hash: info_hash_bytes,
            total_size,
            name: "test".to_string(),
            piece_length: 123,
            piece_hashes: vec![],
        };

        let expected_url = "http://bttracker.debian.org:6969/announce?info_hash=%2C%ED%86%19%66%E9%19%E5%CA%9E%35%D2%7D%C2%3E%0B%02%FB%7F%F8&peer_id=%B1%D0%7B%04%D1%5A%D1%3B%22%B3%59%75%7D%BC%6A%56%3E%89%B2%96&port=6881&uploaded=0&downloaded=0&left=821035008&compact=1&event=started";
        let result_url = build_tracker_url(&mock_torrent, &peer_id_bytes);

        assert_eq!(result_url, expected_url);
    }
}
