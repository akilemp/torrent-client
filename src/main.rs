use std::path::Path;

mod torrent;

fn main() {
    let path = Path::new("debian-13.1.0-amd64-netinst.iso.torrent");

    println!("Attempting to open torrent file: {}", path.display());

    match torrent::open(path) {
        Ok(torrent) => {
            let num_pieces = torrent.info.pieces.len() / 20;
            let info_hash = torrent.info_hash().unwrap();

            println!("\n✅ Successfully decoded torrent metadata:");
            println!("   Announce URL: {}", torrent.announce);
            println!("   Piece Length: {} bytes", torrent.info.piece_length);
            println!("   Number of Pieces: {}", num_pieces);
            println!("   Torrent info hash: {:?}", hex::encode(info_hash));
        }
        Err(e) => {
            eprintln!("\n❌ Failed to process torrent file.");
            match e {
                torrent::TorrentError::Io(io_err) => {
                    eprintln!(
                        "   Reason: File I/O Failure (e.g., file not found or access denied)."
                    );
                    eprintln!("   Details: {:?}", io_err);
                }
                torrent::TorrentError::Bencode(bencode_err) => {
                    eprintln!("   Reason: Bencode Parsing Failure (The file content is corrupt).");
                    eprintln!("   Details: {}", bencode_err);
                }
            }
            std::process::exit(1); // Exit with a non-zero status code to signal failure
        }
    }
}
