use std::path::Path;

mod peer;
mod torrent;
mod tracker;

fn main() {
    let path = Path::new("debian-13.1.0-amd64-netinst.iso.torrent");
    // let path = Path::new("Leap-16.0-online-installer-x86_64-Build171.1.install.iso.torrent");

    println!("Attempting to open torrent file: {}", path.display());

    match torrent::open(path) {
        Ok(torrent) => {
            let num_pieces = torrent.piece_hashes.len();
            let info_hash = torrent.info_hash;

            println!("\n✅ Successfully decoded torrent metadata:");
            println!("   Torrent name: {}", torrent.name);
            println!("   Announce URL: {}", torrent.announce);
            println!("   Piece Length: {} bytes", torrent.piece_length);
            println!("   Number of Pieces: {}", num_pieces);
            println!("   Torrent info hash: {}", hex::encode(info_hash));
            println!("");
            println!("Requesting Peers");
            let peer_id = peer::generate_peer_id();
            println!("   Generated peer id: {}", hex::encode(peer_id));
            println!(
                "   Tracker URL: {}",
                tracker::build_tracker_url(&torrent, &peer_id)
            );
            println!("   Requesting Peers:");
            match tracker::get_peers(&torrent, &peer_id) {
                Ok(peers) => {
                    println!("   Peers: {:?}", peers);
                }
                Err(e) => match e {
                    tracker::TrackerError::HttpClient(error) => println!("{}", error),
                    tracker::TrackerError::BencodeDecode(error) => println!("{}", error),
                    tracker::TrackerError::PeerParse(error) => println!("{}", error),
                },
            }
        }
        Err(e) => {
            eprintln!("\n❌ Failed to process torrent file.");
            match e {
                torrent::BencodeTorrentError::Io(io_err) => {
                    eprintln!(
                        "   Reason: File I/O Failure (e.g., file not found or access denied)."
                    );
                    eprintln!("   Details: {:?}", io_err);
                }
                torrent::BencodeTorrentError::Parsing(bencode_err) => {
                    eprintln!("   Reason: Bencode Parsing Failure (The file content is corrupt).");
                    eprintln!("   Details: {}", bencode_err);
                }
                torrent::BencodeTorrentError::InvalidMetadata(bencode_err) => {
                    eprintln!("   Reason: Invalid torrent metadata.");
                    eprintln!("   Details: {}", bencode_err);
                }
            }
            std::process::exit(1); // Exit with a non-zero status code to signal failure
        }
    }
}
