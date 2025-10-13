use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use std::fs;
use std::path::Path;

// Define a single error enum that can represent all potential errors:
// reading the file (Io) or decoding the content (Bencode).
#[derive(Debug)]
pub enum TorrentError {
    Io(std::io::Error),
    Bencode(serde_bencode::Error),
}

impl From<std::io::Error> for TorrentError {
    fn from(error: std::io::Error) -> Self {
        TorrentError::Io(error)
    }
}

impl From<serde_bencode::Error> for TorrentError {
    fn from(error: serde_bencode::Error) -> Self {
        TorrentError::Bencode(error)
    }
}

// Represents the `info` dictionary in the .torrent file.
// The `#[serde(rename = ...)]` attribute is used because Bencode keys
// can contain spaces, which are not valid in Rust identifiers.
#[derive(Debug, Deserialize, Serialize)]
pub struct BencodeInfo {
    pub name: String,

    #[serde(rename = "piece length")]
    pub piece_length: u64,

    #[serde(with = "serde_bytes")]
    pub pieces: Vec<u8>,

    // The 'length' key is for single-file torrents.
    // The 'files' key is for multi-file torrents.
    // `Option` marks them as optional, since only one can exist.
    pub length: Option<u64>,
    pub files: Option<Vec<MultiFile>>,
}

// Represents a file in a multi-file torrent.
#[derive(Debug, Deserialize, Serialize)]
pub struct MultiFile {
    pub length: u64,
    pub path: Vec<String>,
}

// Represents the top-level structure of the .torrent file.
#[derive(Debug, Deserialize)]
pub struct Torrent {
    pub announce: String,
    pub info: BencodeInfo,

    // Optional fields
    #[serde(rename = "creation date")]
    creation_date: Option<u64>,
    comment: Option<String>,
    #[serde(rename = "created by")]
    created_by: Option<String>,
}

// We need an additional method on Torrent to generate the info hash.
impl Torrent {
    /// Calculates the SHA-1 hash of the bencoded `info` dictionary.
    /// This hash is the unique identifier for the torrent.
    pub fn info_hash(&self) -> Result<[u8; 20], TorrentError> {
        let info_bencoded_bytes = serde_bencode::to_bytes(&self.info)?;
        let mut hasher = Sha1::new();
        hasher.update(&info_bencoded_bytes);

        // The `finalize` method computes the hash and returns it as a generic
        // `GenericArray<u8, U>` which can be converted to the fixed-size array `[u8; 20]`.
        let hash_result = hasher.finalize();

        // The Sha1 hash is 20 bytes long.
        let mut info_hash = [0u8; 20];
        info_hash.copy_from_slice(&hash_result[..]);

        Ok(info_hash)
    }
}

pub fn open(torrent_path: &Path) -> Result<Torrent, TorrentError> {
    let torrent_bytes = fs::read(torrent_path)?;
    let torrent: Torrent = serde_bencode::from_bytes(&torrent_bytes)?;
    return Ok(torrent);
}
