#![allow(dead_code)]

#[derive(Debug)]
pub enum BitfieldError {
    InvalidLength,
}

pub struct Bitfield {
    bits: Vec<u8>,
    piece_count: usize,
}

impl Bitfield {
    pub fn new(piece_count: usize) -> Self {
        let byte_count = piece_count.div_ceil(8);
        Self {
            bits: vec![0; byte_count],
            piece_count,
        }
    }

    pub fn has_piece(&self, index: usize) -> bool {
        if index >= self.piece_count {
            return false;
        }

        let byte_index = index / 8;
        let bit_index = 7 - (index % 8);
        (self.bits[byte_index] >> bit_index) & 1 == 1
    }

    pub fn set_piece(&mut self, index: usize) {
        if index >= self.piece_count {
            return;
        }

        let byte_index = index / 8;
        let bit_index = 7 - (index % 8);
        self.bits[byte_index] |= 1 << bit_index;
    }

    pub fn from_bytes(data: Vec<u8>, piece_count: usize) -> Result<Self, BitfieldError> {
        let expect_len = piece_count.div_ceil(8);
        if data.len() != expect_len {
            return Err(BitfieldError::InvalidLength);
        }

        Ok(Self {
            bits: data,
            piece_count,
        })
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        self.bits.clone()
    }
}

// --- Tests ---

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_bitfield_all_empty() {
        let bitfield = Bitfield::new(10); // 10 pieces
        for i in 0..10 {
            assert!(!bitfield.has_piece(i));
        }
    }

    #[test]
    fn set_correct_bits() {
        let mut bf = Bitfield::new(16);

        bf.set_piece(0);
        assert_eq!(bf.bits.clone(), vec![0b1000_0000, 0b0000_0000]);
        bf.set_piece(2);
        assert_eq!(bf.bits.clone(), vec![0b1010_0000, 0b0000_0000]);
        bf.set_piece(7);
        assert_eq!(bf.bits.clone(), vec![0b1010_0001, 0b0000_0000]);
        bf.set_piece(10);
        assert_eq!(bf.bits.clone(), vec![0b1010_0001, 0b0010_0000]);
    }

    #[test]
    fn set_and_check_piece() {
        let mut bitfield = Bitfield::new(10);
        bitfield.set_piece(3);

        assert!(bitfield.has_piece(3));
        assert!(!bitfield.has_piece(2));
        assert!(!bitfield.has_piece(4));
    }

    #[test]
    fn from_raw_bytes_and_to_bytes() {
        let raw = vec![0b1010_0000]; // Only 4 bits relevant
        let bf = Bitfield::from_bytes(raw.clone(), 4).unwrap();

        assert!(bf.has_piece(0));
        assert!(!bf.has_piece(1));
        assert!(bf.has_piece(2));
        assert!(!bf.has_piece(3));

        assert_eq!(bf.to_bytes(), raw);
    }
}
