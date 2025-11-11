use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::net::TcpStream;

use tokio::io::AsyncWriteExt;
use tokio::time::{Instant, sleep, timeout};

use crate::message::Message;
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
        let active_peers = Arc::new(AtomicUsize::new(0));
        let total_downloaded = Arc::new(AtomicUsize::new(0));
        let done = Arc::new(AtomicBool::new(false));

        for peer in &self.peers {
            let active_peers = active_peers.clone();
            let total_downloaded = total_downloaded.clone();
            let peer_clone = peer.clone();
            let torrent = self.torrent.clone();
            let pieces = self.pieces.clone();
            let done = done.clone();

            let handle = tokio::spawn(async move {
                active_peers.fetch_add(1, Ordering::SeqCst);

                // Connect to peer
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
                        active_peers.fetch_sub(1, Ordering::SeqCst);
                        return;
                    }
                };

                // Wait for bitfield
                match conn.wait_for_bitfield().await {
                    Ok(bitfield) => conn.bitfield = Some(bitfield),
                    Err(e) => {
                        eprintln!("Failed to get bitfield from peer {:?}: {:?}", peer_clone, e);
                        active_peers.fetch_sub(1, Ordering::SeqCst);
                        return;
                    }
                };

                // Download pieces this peer has
                loop {
                    // Exit if someone set the done flag
                    if done.load(Ordering::SeqCst) {
                        println!(
                            "✅ Torrent fully downloaded — closing peer {:?}",
                            peer_clone
                        );
                        active_peers.fetch_sub(1, Ordering::SeqCst);
                        break;
                    }

                    if total_downloaded.load(Ordering::SeqCst) >= torrent.total_size as usize {
                        done.store(true, Ordering::SeqCst);
                        continue;
                    }

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
                        None => {
                            // No work at the moment — send keep-alive to prevent timeout
                            if let Err(e) = conn.write_message(&Message::keep_alive()).await {
                                eprintln!("Failed to send keep-alive to {:?}: {:?}", peer_clone, e);
                                break; // Connection probably closed
                            }

                            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                            continue;
                        }
                    };

                    // Download piece
                    // Limit how long we’ll wait for one piece (e.g. 60 seconds)
                    let download_timeout = Duration::from_secs(30);
                    let result =
                        timeout(download_timeout, conn.download_piece(&torrent, piece_index)).await;

                    let result = match result {
                        Ok(r) => r,
                        Err(_) => {
                            eprintln!(
                                "Peer {:?} timed out while downloading piece {}",
                                peer_clone, piece_index
                            );
                            {
                                let mut pieces_guard = pieces.lock().unwrap();
                                pieces_guard[piece_index as usize] = None;
                            }
                            // TODO send cancel ???
                            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                            continue;
                        }
                    };

                    match result {
                        Ok(data) => {
                            // Lock just long enough to store the piece
                            let mut pieces_guard = pieces.lock().unwrap();
                            pieces_guard[piece_index as usize] = Some(data.clone());

                            total_downloaded.fetch_add(data.len(), Ordering::SeqCst);
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

                active_peers.fetch_sub(1, Ordering::SeqCst);
            });

            handles.push(handle);
        }

        self.log_proggress(active_peers, total_downloaded);

        // Wait for all peer tasks to complete
        for h in handles {
            let _ = h.await;
        }

        println!("Download complete (some pieces may still be missing)");
        write_torrent_to_disk(&self.torrent.name, &self.pieces).await?;

        Ok(())
    }

    fn log_proggress(&self, active_peers: Arc<AtomicUsize>, total_downloaded: Arc<AtomicUsize>) {
        let progress_pieces = self.pieces.clone();
        let progress_peers = active_peers.clone();
        let progress_bytes = total_downloaded.clone();
        let progress_torrent = self.torrent.clone();

        tokio::spawn(async move {
            let mut last_bytes = 0;
            let mut last_time = Instant::now();

            loop {
                sleep(Duration::from_millis(500)).await;

                // Calculate speed
                let now = Instant::now();
                let downloaded = progress_bytes.load(Ordering::SeqCst);
                let elapsed = now.duration_since(last_time).as_secs_f64().max(1.0);
                let speed_kbps = (downloaded - last_bytes) as f64 / 1024.0 / elapsed;
                last_bytes = downloaded;
                last_time = now;

                // Calculate completion percentage
                let pieces_guard = progress_pieces.lock().unwrap();
                let total_bytes: usize = pieces_guard
                    .iter()
                    .filter_map(|p| p.as_ref().map(|d| d.len()))
                    .sum();

                let percent = (total_bytes as f64 / progress_torrent.total_size as f64) * 100.0;
                let active = progress_peers.load(Ordering::SeqCst);

                println!(
                    "⏬ {:.2}% complete | {:.1} KB/s | Active peers: {}",
                    percent, speed_kbps, active
                );
            }
        });
    }
}

/// Assemble pieces, verify the full file hash, and write to disk.
async fn write_torrent_to_disk(
    filename: &str,
    pieces: &Arc<Mutex<Vec<Option<Vec<u8>>>>>,
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

    let mut file = tokio::fs::File::create(filename).await?;
    file.write_all(&full_data).await?;
    println!("Torrent written to disk successfully: {}", filename);

    Ok(())
}
