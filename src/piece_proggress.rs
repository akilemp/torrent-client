#![allow(dead_code)]

pub struct PieceProgress {
    pub index: u32,
    pub total_length: usize,
    pub block_size: usize,
    pub data: Vec<u8>,
    pub received_blocks: Vec<bool>,
    pub requested_blocks: Vec<bool>,
    pub received: usize,
}

impl PieceProgress {
    pub const BLOCK_SIZE: usize = 16 * 1024;
    pub const MAX_PIPELINE: usize = 5;

    pub fn new(index: u32, total_length: usize) -> Self {
        let num_blocks = total_length.div_ceil(PieceProgress::BLOCK_SIZE);
        Self {
            index,
            total_length,
            block_size: PieceProgress::BLOCK_SIZE,
            data: vec![0u8; total_length],
            received_blocks: vec![false; num_blocks],
            requested_blocks: vec![false; num_blocks],
            received: 0,
        }
    }

    pub fn is_complete(&self) -> bool {
        self.received >= self.total_length
    }

    pub fn mark_block(&mut self, begin: usize, block: &[u8]) {
        let block_index = begin / self.block_size;
        if begin + block.len() <= self.total_length && !self.received_blocks[block_index] {
            self.data[begin..begin + block.len()].copy_from_slice(block);
            self.received_blocks[block_index] = true;
            self.received += block.len();
        }
    }

    pub fn next_requests(&mut self, max_requests: usize) -> Vec<(u32, u32)> {
        let mut requests = Vec::new();
        let num_blocks = self.received_blocks.len();

        for i in 0..num_blocks {
            if !self.received_blocks[i] && !self.requested_blocks[i] {
                self.requested_blocks[i] = true;

                let offset = (i * self.block_size) as u32;
                let length =
                    std::cmp::min(self.block_size, self.total_length - (i * self.block_size))
                        as u32;

                requests.push((offset, length));
                if requests.len() >= max_requests {
                    break;
                }
            }
        }

        requests
    }

    pub fn verify(&self, expected_hash: &[u8]) -> bool {
        use sha1::{Digest, Sha1};
        let mut hasher = Sha1::new();
        hasher.update(&self.data);
        let result: [u8; 20] = hasher.finalize().into();
        result.as_ref() == expected_hash
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_initial_state() {
        let progress = PieceProgress::new(0, 32 * 1024);
        assert_eq!(progress.index, 0);
        assert_eq!(progress.total_length, 32 * 1024);
        assert_eq!(progress.received, 0);
        assert!(!progress.is_complete());
    }

    #[test]
    fn test_mark_block_updates_state() {
        let mut progress = PieceProgress::new(0, 32 * 1024);

        let block_data = vec![1u8; PieceProgress::BLOCK_SIZE];
        progress.mark_block(0, &block_data);

        assert_eq!(progress.received, PieceProgress::BLOCK_SIZE);
        assert_eq!(&progress.data[..PieceProgress::BLOCK_SIZE], &block_data[..]);
    }

    #[test]
    fn test_next_requests_does_not_repeat() {
        let mut progress = PieceProgress::new(0, 64 * 1024);

        let requests = progress.next_requests(2);
        assert_eq!(requests.len(), 2);
        assert_eq!(progress.requested_blocks[0], true);
        assert_eq!(progress.requested_blocks[1], true);

        let next = progress.next_requests(2);
        assert_eq!(next.len(), 2);
        assert_eq!(progress.requested_blocks[2], true);
        assert_eq!(progress.requested_blocks[3], true);
    }

    #[test]
    fn test_is_complete() {
        let mut progress = PieceProgress::new(0, 2 * PieceProgress::BLOCK_SIZE);
        let block_data = vec![0xFF; PieceProgress::BLOCK_SIZE];

        progress.mark_block(0, &block_data);
        assert!(!progress.is_complete());

        progress.mark_block(PieceProgress::BLOCK_SIZE, &block_data);
        assert!(progress.is_complete());
    }
}
