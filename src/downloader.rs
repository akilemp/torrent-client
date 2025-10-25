use std::sync::{Arc, Mutex};

use tokio::net::TcpStream;

use sha1::{Digest, Sha1};
use tokio::io::AsyncWriteExt;

use crate::message::{Message, MessageId};
use crate::peer::Peer;
use crate::peer_connection::PeerConnection;
use crate::torrent::VerifiedTorrent;

#[derive(Debug)]
pub struct Downloader {
    pub torrent: Arc<VerifiedTorrent>,
    pub peers: Vec<Peer>,
    pub pieces: Arc<Mutex<Vec<Option<Vec<u8>>>>>, // downloaded pieces
}

impl Downloader {
    pub fn new(torrent: VerifiedTorrent, peers: Vec<Peer>) -> Self {
        let num_pieces = torrent.piece_hashes.len();
        Self {
            torrent: Arc::new(torrent),
            peers,
            pieces: Arc::new(Mutex::new(vec![None; num_pieces])),
        }
    }

    /// Start concurrent downloading
    pub async fn start(&self, client_peer_id: [u8; 20]) -> anyhow::Result<()> {
        let mut handles = Vec::new();

        for peer in &self.peers {
            let peer_clone = peer.clone();
            let torrent = self.torrent.clone();
            let pieces = self.pieces.clone();

            let handle = tokio::spawn(async move {
                // 1️⃣ Connect to peer
                let mut conn = match PeerConnection::<TcpStream>::connect(
                    &peer_clone,
                    torrent.info_hash,
                    client_peer_id,
                )
                .await
                {
                    Ok(c) => c,
                    Err(e) => {
                        eprintln!("Failed to connect to peer {:?}: {:?}", peer_clone, e);
                        return;
                    }
                };

                // 2️⃣ Wait for bitfield
                match conn.wait_for_bitfield().await {
                    Ok(bitfield) => conn.bitfield = Some(bitfield),
                    Err(e) => {
                        eprintln!("Failed to get bitfield from peer {:?}: {:?}", peer_clone, e);
                        return;
                    }
                };



                // 3️⃣ Download pieces this peer has
                loop {
                    let piece_index = {
                        let mut pieces_guard = pieces.lock().unwrap();

                        if let Some(peer_bitfield) = &conn.bitfield {
                            // Find the first piece the peer has that is not yet started
                            if let Some((i, _)) = peer_bitfield
                                .iter()
                                .find(|(i, has_piece)| *has_piece && pieces_guard[*i].is_none())
                                {
                                    // Mark as in-progress to reserve it for this peer
                                    pieces_guard[i] = Some(Vec::new());
                                    Some(i as u32)
                                } else {
                                    None
                                }
                        } else {
                            None
                        }
                    };

                    let piece_index = match piece_index {
                        Some(idx) => idx,
                        None => break, // no more pieces for this peer
                    };

                    // Download piece
                    let result = conn.download_piece(&torrent, piece_index).await;

                    match result {
                        Ok(data) => {
                            // Lock just long enough to store the piece
                            let mut pieces_guard = pieces.lock().unwrap();
                            pieces_guard[piece_index as usize] = Some(data);

                            let percent = torrent_progress(&pieces_guard);
                            println!(
                                "[{:.2}% complete] Downloaded piece {} from peer {:?}",
                                percent, piece_index, peer_clone
                            );
                        }
                        Err(e) => {
                            eprintln!(
                                "Failed to download piece {} from peer {:?}: {:?}",
                                piece_index, peer_clone, e
                            );

                            // Free the piece so another peer can try it
                            {
                                let mut pieces_guard = pieces.lock().unwrap();
                                pieces_guard[piece_index as usize] = None;
                            }

                            break;
                        }
                    }
                }
            });

            handles.push(handle);
        }

        // Wait for all peer tasks to complete
        for h in handles {
            let _ = h.await;
        }

        println!("Download complete (some pieces may still be missing)");
        write_torrent_to_disk(
            &self.torrent.name, // or some filename
            &self.pieces,
            self.torrent.info_hash,
        )
        .await?;

        Ok(())
    }
}

/// Compute overall download progress as a percentage
fn torrent_progress(pieces: &Vec<Option<Vec<u8>>>) -> f64 {
    let total = pieces.len();
    let downloaded = pieces.iter().filter(|p| p.is_some()).count();
    (downloaded as f64 / total as f64) * 100.0
}

/// Assemble pieces, verify the full file hash, and write to disk.
async fn write_torrent_to_disk(
    filename: &str,
    pieces: &Arc<Mutex<Vec<Option<Vec<u8>>>>>,
    expected_info_hash: [u8; 20],
) -> anyhow::Result<()> {
    let pieces_guard = pieces.lock().unwrap(); // lock the mutex
    let mut full_data = Vec::new();

    for piece in pieces_guard.iter() {
        if let Some(block) = piece {
            full_data.extend_from_slice(block);
        } else {
            return Err(anyhow::anyhow!("Missing piece, cannot write file yet"));
        }
    }

    // Check SHA1 hash
    let mut hasher = Sha1::new();
    hasher.update(&full_data);
    let computed_hash = hasher.finalize(); // this gives a GenericArray<u8, 20>

    if computed_hash[..] != expected_info_hash[..] {
        return Err(anyhow::anyhow!("Torrent hash mismatch! Not writing file."));
    }

    let mut file = tokio::fs::File::create(filename).await?;
    file.write_all(&full_data).await?;
    println!("Torrent written to disk successfully: {}", filename);

    Ok(())
}
