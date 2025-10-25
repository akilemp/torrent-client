#![allow(dead_code)]

#[derive(Debug)]
pub struct Bitfield {
    bytes: Vec<u8>,
}

impl Bitfield {
    /// Creates a new Bitfield with enough bytes to hold `byte_len * 8` bits.
    pub fn new(byte_len: usize) -> Self {
        Self {
            bytes: vec![0; byte_len],
        }
    }

    /// Returns true if the bit at `index` is set.
    pub fn has_piece(&self, index: usize) -> bool {
        let byte_index = index / 8;
        let bit_index = 7 - (index % 8);
        self.bytes
            .get(byte_index)
            .map(|b| (b >> bit_index) & 1 == 1)
            .unwrap_or(false)
    }

    /// Sets the bit at `index` to 1.
    pub fn set_piece(&mut self, index: usize) {
        let byte_index = index / 8;
        let bit_index = 7 - (index % 8);
        if let Some(byte) = self.bytes.get_mut(byte_index) {
            *byte |= 1 << bit_index;
        }
    }

    /// Construct a Bitfield from raw bytes.
    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        Self { bytes }
    }

    /// Return the underlying bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        self.bytes.clone()
    }

    /// Returns an iterator over `(index, has_piece)` for all bits.
    pub fn iter(&self) -> impl Iterator<Item = (usize, bool)> + '_ {
        let piece_count = self.bytes.len() * 8;
        (0..piece_count).map(move |i| (i, self.has_piece(i)))
    }

    /// Returns an iterator over indices of set bits only.
    pub fn iter_set(&self) -> impl Iterator<Item = usize> + '_ {
        self.iter()
            .filter_map(|(i, has)| if has { Some(i) } else { None })
    }
}

// --- Tests ---
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_bitfield_all_empty() {
        let bitfield = Bitfield::new(2); // 16 bits
        for i in 0..16 {
            assert!(!bitfield.has_piece(i));
        }
    }

    #[test]
    fn set_and_check_piece() {
        let mut bf = Bitfield::new(2);
        bf.set_piece(3);
        bf.set_piece(10);

        assert!(bf.has_piece(3));
        assert!(!bf.has_piece(2));
        assert!(bf.has_piece(10));
    }

    #[test]
    fn from_bytes_and_to_bytes() {
        let raw = vec![0b1010_0000];
        let bf = Bitfield::from_bytes(raw.clone());

        assert!(bf.has_piece(0));
        assert!(!bf.has_piece(1));
        assert!(bf.has_piece(2));
        assert!(!bf.has_piece(3));
        assert_eq!(bf.to_bytes(), raw);
    }

    #[test]
    fn iter_and_iter_set() {
        let mut bf = Bitfield::new(1);
        bf.set_piece(0);
        bf.set_piece(3);
        bf.set_piece(7);

        let all: Vec<_> = bf.iter().collect();
        assert_eq!(
            all,
            vec![
                (0, true),
                (1, false),
                (2, false),
                (3, true),
                (4, false),
                (5, false),
                (6, false),
                (7, true)
            ]
        );

        let set_indices: Vec<_> = bf.iter_set().collect();
        assert_eq!(set_indices, vec![0, 3, 7]);
    }
}
