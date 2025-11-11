torrent-client

Overview

A simple, BitTorrent client written entirely in Rust. This project is a foundational, leech-only implementation that is inspired by jse.li's blog post 'Writing a BitTorrent Client', and is based on BitTorrent Specification (BEP 3). It is built primarily for educational purposes to understand the core protocol.

Features / Limitations

* Built in Rust.

* Tracker only Support: Connects to HTTP/UDP trackers to discover peers.

* Downloads: Downloads are verified via SHA-1 hashing before saving.

* Leech only: Focuses only on downloading content, not seeding.

Getting Started

Prerequisites

You must have Rust and Cargo (Rust's package manager) installed.

Build and Run

1. Clone the repository:

git clone https://github.com/[GITHUB_USERNAME]/torrent-client.git
cd torrent-client


2. Compile the binary:

cargo build --release


The executable, torrent-client, is now available in the target/release/ directory.

Usage

To start a download, run the client from your command line:

./target/release/torrent-client
