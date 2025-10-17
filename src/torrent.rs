use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use std::fs;
use std::path::Path;

const SHA1_SIZE: usize = 20;

#[derive(Debug)]
pub enum BencodeTorrentError {
    Io(std::io::Error),
    Parsing(serde_bencode::Error),
    InvalidMetadata(String),
}

impl From<std::io::Error> for BencodeTorrentError {
    fn from(error: std::io::Error) -> Self {
        BencodeTorrentError::Io(error)
    }
}

impl From<serde_bencode::Error> for BencodeTorrentError {
    fn from(error: serde_bencode::Error) -> Self {
        BencodeTorrentError::Parsing(error)
    }
}

// Represents the `info` dictionary in the .torrent file.
// The `#[serde(rename = ...)]` attribute is used because Bencode keys
// can contain spaces, which are not valid in Rust identifiers.
#[derive(Debug, Deserialize, Serialize)]
struct BencodeInfo {
    name: String,

    // The length of each piece.
    #[serde(rename = "piece length")]
    piece_length: u64,

    // This is a single byte vector containing all concatenated 20-byte SHA-1 hashes.
    #[serde(with = "serde_bytes")]
    pieces: Vec<u8>,

    // The 'length' key is for single-file torrents.
    // The 'files' key is for multi-file torrents.
    // `Option` marks them as optional, since only one can exist.
    length: Option<u64>,
    files: Option<Vec<MultiFile>>,
}

impl BencodeInfo {
    /// Splits the concatenated `pieces` byte vector into a vector of individual
    /// 20-byte SHA-1 hashes.
    ///
    /// Returns an error if the total length is not an exact multiple of 20.
    fn piece_hashes(&self) -> Result<Vec<[u8; SHA1_SIZE]>, BencodeTorrentError> {
        if !self.pieces.len().is_multiple_of(SHA1_SIZE) {
            return Err(BencodeTorrentError::InvalidMetadata(
                "The length of the 'pieces' field is not a multiple of 20 bytes.".to_string(),
            ));
        }

        let (hash_slices, _remainder) = self.pieces.as_chunks::<SHA1_SIZE>();
        let hashes = hash_slices.to_vec();

        Ok(hashes)
    }

    /// Calculates the SHA-1 hash of the bencoded `info` dictionary.
    /// This hash is the unique identifier for the torrent.
    fn info_hash(&self) -> Result<[u8; SHA1_SIZE], BencodeTorrentError> {
        let info_bencoded_bytes = serde_bencode::to_bytes(&self)?;
        let mut hasher = Sha1::new();
        hasher.update(&info_bencoded_bytes);

        // The `finalize` method computes the hash and returns it as a generic
        // `GenericArray<u8, U>` which can be converted to the fixed-size array `[u8; 20]`.
        let hash_result = hasher.finalize();

        let mut info_hash = [0u8; SHA1_SIZE];
        info_hash.copy_from_slice(&hash_result[..]);

        Ok(info_hash)
    }

    /// Calculates the total size of the torrent date in bytes.
    /// Handles both single-file and multi-file torrent structures.
    fn total_size(&self) -> Result<u64, BencodeTorrentError> {
        if let Some(length) = self.length {
            return Ok(length);
        }

        if let Some(files) = &self.files {
            let total: u64 = files.iter().map(|f| f.length).sum();
            return Ok(total);
        }

        Err(BencodeTorrentError::InvalidMetadata(
            "Torrent info dictionary is missing both 'length' and 'files' keys.".to_string(),
        ))
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct MultiFile {
    pub length: u64,
    pub path: Vec<String>,
}

// Represents the top-level structure of the .torrent file.
#[derive(Debug, Deserialize)]
struct BencodeTorrent {
    pub announce: String,
    pub info: BencodeInfo,
    // Optional fields
    #[serde(rename = "creation date")]
    _creation_date: Option<u64>,
    _comment: Option<String>,
    #[serde(rename = "created by")]
    _created_by: Option<String>,
}

pub struct VerifiedTorrent {
    pub announce: String,
    pub info_hash: [u8; SHA1_SIZE],
    pub name: String,
    pub piece_length: u64,
    pub piece_hashes: Vec<[u8; SHA1_SIZE]>,
    pub total_size: u64,
}

pub fn open(torrent_path: &Path) -> Result<VerifiedTorrent, BencodeTorrentError> {
    let torrent_bytes = fs::read(torrent_path)?;
    let torrent: BencodeTorrent = serde_bencode::from_bytes(&torrent_bytes)?;
    let info_hash = torrent.info.info_hash()?;
    let piece_hashes = torrent.info.piece_hashes()?;
    let total_size = torrent.info.total_size()?;

    let verified_torrent: VerifiedTorrent = VerifiedTorrent {
        announce: torrent.announce,
        info_hash,
        name: torrent.info.name,
        piece_length: torrent.info.piece_length,
        piece_hashes,
        total_size,
    };

    Ok(verified_torrent)
}
