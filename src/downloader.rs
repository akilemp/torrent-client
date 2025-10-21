#![allow(dead_code)]

use std::io::{Read, Write};

use crate::piece_proggress::PieceProgress;

use crate::bitfield::Bitfield;
use crate::message::{Message, MessageError, MessageId};
use crate::peer_connection::PeerConnection;
use crate::torrent::VerifiedTorrent; // your struct

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

pub struct Downloader<S: Read + Write> {
    conn: PeerConnection<S>,
    torrent: VerifiedTorrent,
    peer_bitfield: Bitfield,
}

impl<S: Read + Write> Downloader<S> {
    pub fn new(conn: PeerConnection<S>, torrent: VerifiedTorrent, peer_bitfield: Bitfield) -> Self {
        Self {
            conn,
            torrent,
            peer_bitfield,
        }
    }

    pub fn download_piece(&mut self, piece_index: u32) -> Result<Vec<u8>, DownloadError> {
        if !self.peer_bitfield.has_piece(piece_index as usize) {
            return Err(DownloadError::PieceNotAvailable);
        }

        self.conn.write_message(&Message::interested())?;
        self.wait_for_unchoke()?;

        let piece_len = self.piece_length(piece_index);
        let mut progress = PieceProgress::new(piece_index, piece_len, PieceProgress::BLOCK_SIZE);

        while !progress.is_complete() {
            for (offset, length) in progress.next_requests(PieceProgress::MAX_PIPELINE) {
                let req = Message::request(piece_index, offset, length);
                self.conn.write_message(&req)?;
            }

            let msg = self.conn.read_message()?;
            if let Some(MessageId::Piece) = msg.id {
                let (index, begin, block) = Message::parse_piece_payload(&msg.payload)?;
                if index == piece_index {
                    progress.mark_block(begin as usize, &block);
                }
            }
        }

        if !self.verify_piece(piece_index, &progress.data) {
            return Err(DownloadError::InvalidHash);
        }

        Ok(progress.data)
    }

    fn wait_for_unchoke(&mut self) -> Result<(), DownloadError> {
        loop {
            let msg = self.conn.read_message()?;
            match msg.id {
                Some(MessageId::Unchoke) => return Ok(()),
                Some(MessageId::Choke) => return Err(DownloadError::PeerChoked),
                _ => continue,
            }
        }
    }

    fn piece_length(&self, index: u32) -> usize {
        let index = index as usize;
        let full_piece_len = self.torrent.piece_length as usize;

        if index == self.torrent.piece_hashes.len() - 1 {
            let total = self.torrent.total_size as usize;
            let remainder = total % full_piece_len;
            if remainder != 0 {
                return remainder;
            }
        }

        full_piece_len
    }

    fn verify_piece(&self, index: u32, data: &[u8]) -> bool {
        use sha1::{Digest, Sha1};

        let mut hasher = Sha1::new();
        hasher.update(data);
        let result = hasher.finalize();

        result.as_ref() == self.torrent.piece_hashes[index as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha1::{Digest, Sha1};
    use std::io::{Cursor, Read, Write};

    struct FakeStream {
        read_data: Cursor<Vec<u8>>,
        written_data: Vec<u8>,
    }

    impl Read for FakeStream {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            self.read_data.read(buf)
        }
    }

    impl Write for FakeStream {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.written_data.extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn test_download_piece_successful() {
        // Simulate: Unchoke + a Piece message with data
        let piece_index = 0;
        let block_offset = 0;
        let block_data = vec![1u8; PieceProgress::BLOCK_SIZE];
        let hash = Sha1::digest(&block_data);
        let mut piece_hash = [0u8; 20];
        piece_hash.copy_from_slice(&hash[..]);
        let payload = build_piece_payload(piece_index, block_offset, &block_data);
        let piece_msg = build_message(Some(MessageId::Piece), &payload);
        let unchoke_msg = build_message(Some(MessageId::Unchoke), &[]);

        let input_data = [&unchoke_msg[..], &piece_msg[..]].concat();

        let stream = FakeStream {
            read_data: Cursor::new(input_data),
            written_data: Vec::new(),
        };

        let torrent = VerifiedTorrent {
            announce: String::new(),
            info_hash: [0u8; 20],
            name: String::from("test"),
            piece_length: PieceProgress::BLOCK_SIZE as u64,
            piece_hashes: vec![piece_hash],
            total_size: PieceProgress::BLOCK_SIZE as u64,
        };

        let mut bitfield = Bitfield::new(1);
        bitfield.set_piece(0); // simulate that peer has piece 0

        let conn = PeerConnection::_new(stream, [0; 20]);

        let mut downloader = Downloader {
            conn,
            torrent,
            peer_bitfield: bitfield,
            // Set peer bitfield, etc.
        };

        let result = downloader.download_piece(0);

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), block_data);
    }

    fn build_message(id: Option<MessageId>, payload: &[u8]) -> Vec<u8> {
        let msg = Message {
            id: id,
            payload: payload.to_vec(),
        };
        msg.to_bytes()
    }

    fn build_piece_payload(index: u32, begin: u32, block: &[u8]) -> Vec<u8> {
        let mut payload = Vec::new();
        payload.extend_from_slice(&index.to_be_bytes());
        payload.extend_from_slice(&begin.to_be_bytes());
        payload.extend_from_slice(block);
        payload
    }
}
