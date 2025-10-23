#![allow(dead_code)]

use tokio::io::{AsyncRead, AsyncWrite};

use crate::bitfield::Bitfield;
use crate::peer_connection::PeerConnection;
use crate::torrent::VerifiedTorrent;


pub struct Downloader<S: AsyncRead + AsyncWrite + Unpin> {
    pub conn: PeerConnection<S>,
    pub torrent: VerifiedTorrent,
    pub peer_bitfield: Bitfield,
}

impl<S: AsyncRead + AsyncWrite + Unpin> Downloader<S> {
    pub fn new(conn: PeerConnection<S>, torrent: VerifiedTorrent, peer_bitfield: Bitfield) -> Self {
        Self {
            conn,
            torrent,
            peer_bitfield,
        }
    }

    pub async fn download_piece(&mut self, piece_index: u32) -> Result<Vec<u8>, crate::peer_connection::DownloadError> {
        self.conn.download_piece(&self.torrent, &self.peer_bitfield, piece_index ).await
    }
}
