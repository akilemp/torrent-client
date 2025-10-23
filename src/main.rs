use std::{ path::Path};


mod bitfield;
mod downloader;
mod handshake;
mod message;
mod peer;
mod peer_connection;
mod piece_proggress;
mod torrent;
mod tracker;

#[tokio::main]
async fn main() ->Result<(), Box<dyn std::error::Error>> {
    let path = Path::new("debian-13.1.0-amd64-netinst.iso.torrent");
    // let path = Path::new("Leap-16.0-online-installer-x86_64-Build171.1.install.iso.torrent");

    println!("Attempting to open torrent file: {}", path.display());



    Ok(())
}
