# torrent-client

## Overview

A simple, BitTorrent client written entirely in **Rust**. This project is a foundational, **leech-only** implementation that is inspired by jse.li's blog post 'Writing a BitTorrent Client', and is based on BitTorrent Specification (BEP 3). It is built primarily for educational purposes to understanding core protocol.

---

## Features / Limitations

* Built in **Rust**.

* **Tracker only:** Connects to HTTP/UDP trackers to discover peers.

* **Leech only:** Focuses only on downloading content, not seeding.

---

## Getting Started

### Prerequisites

You must have **Rust** and **Cargo** installed.

### Build and Run

1.  **Clone the repository:**

    ```bash
    git clone git@github.com:akilemp/torrent-client.git
    cd torrent-client
    ```

2.  **Compile the binary:**

    ```bash
    cargo build --release
    ```

    The executable, `torrent-client`, is now available in the `target/release/` directory.

---

## Usage

To start a download, run the client from your command line:

```bash
./target/release/torrent-client