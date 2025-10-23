#![allow(dead_code)]

use std::convert::TryFrom;
use std::fmt;

#[derive(Debug)]
pub enum MessageError {
    InvalidLength,
    IncompleteData,
    InvalidMessageId(u8),
    IoError(std::io::Error),
    InvalidPayload,
}

impl From<std::io::Error> for MessageError {
    fn from(value: std::io::Error) -> MessageError {
        MessageError::IoError(value)
    }
}

impl fmt::Display for MessageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MessageError::InvalidLength => write!(f, "Invalid message length (too short)"),
            MessageError::IncompleteData => write!(f, "Not enogh data for declared length"),
            MessageError::InvalidMessageId(id) => write!(f, "Invalid message ID: {}", id),
            MessageError::IoError(e) => {
                write!(f, "Error while reading message from TcpStream: {}", e)
            }
            MessageError::InvalidPayload => {
                write!(f, "Invalid payload (too short)")
            }
        }
    }
}

impl std::error::Error for MessageError {}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageId {
    Choke = 0,
    Unchoke = 1,
    Interested = 2,
    NotInterested = 3,
    Have = 4,
    Bitfield = 5,
    Request = 6,
    Piece = 7,
    Cancel = 8,
}

impl TryFrom<u8> for MessageId {
    type Error = MessageError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Choke),
            1 => Ok(Self::Unchoke),
            2 => Ok(Self::Interested),
            3 => Ok(Self::NotInterested),
            4 => Ok(Self::Have),
            5 => Ok(Self::Bitfield),
            6 => Ok(Self::Request),
            7 => Ok(Self::Piece),
            8 => Ok(Self::Cancel),
            other => Err(MessageError::InvalidMessageId(other)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub id: Option<MessageId>,
    pub payload: Vec<u8>,
}

impl Message {
    pub fn to_bytes(&self) -> Vec<u8> {
        match self.id {
            None => {
                vec![0, 0, 0, 0]
            }
            Some(id) => {
                let lenght = 1 + self.payload.len();
                let mut buf = Vec::with_capacity(4 + lenght);

                // Write 4-byte length (big-endian)
                buf.extend_from_slice(&(lenght as u32).to_be_bytes());

                buf.push(id as u8);

                buf.extend_from_slice(&self.payload);

                buf
            }
        }
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self, MessageError> {
        if data.len() < 4 {
            return Err(MessageError::InvalidLength);
        }

        let msg_len = u32::from_be_bytes([data[0], data[1], data[2], data[3]]) as usize;

        if msg_len == 0 {
            return Ok(Message {
                id: None,
                payload: Vec::new(),
            });
        }

        if data.len() != 4 + msg_len {
            return Err(MessageError::IncompleteData);
        }

        let id = MessageId::try_from(data[4])?;

        let payload = match id {
            // Messages without payload
            MessageId::Choke
            | MessageId::Unchoke
            | MessageId::Interested
            | MessageId::NotInterested => Vec::new(),

            // Messages with payload
            MessageId::Have
            | MessageId::Bitfield
            | MessageId::Request
            | MessageId::Piece
            | MessageId::Cancel => data[5..].to_vec(),
        };

        Ok(Message {
            id: Some(id),
            payload,
        })
    }

    pub fn interested() -> Message {
        Message {
            id: Some(MessageId::Interested),
            payload: Vec::new(),
        }
    }

    pub fn keep_alive() -> Message {
        Message { id: None, payload: vec![] }
    }

    pub fn request(index: u32, begin: u32, length: u32) -> Message {
        let id = MessageId::Request;
        let mut payload = Vec::with_capacity(12);
        payload.extend_from_slice(&index.to_be_bytes());
        payload.extend_from_slice(&begin.to_be_bytes());
        payload.extend_from_slice(&length.to_be_bytes());
        Message {
            id: Some(id),
            payload,
        }
    }

    pub fn have(piece_index: u32) ->Message {
        let id = MessageId::Have;
        let payload = piece_index.to_be_bytes().to_vec();
        Message { id: Some(id), payload: payload }
    }

    pub fn parse_piece_payload(payload: &[u8]) -> Result<(u32, u32, Vec<u8>), MessageError> {
        if payload.len() < 8 {
            return Err(MessageError::InvalidPayload);
        }

        let index = u32::from_be_bytes([payload[0], payload[1], payload[2], payload[3]]);
        let begin = u32::from_be_bytes([payload[4], payload[5], payload[6], payload[7]]);
        let block = payload[8..].to_vec();

        Ok((index, begin, block))
    }
}

// --- Tests ---

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serialize_have_message() {
        let msg = Message {
            id: Some(MessageId::Have),
            payload: vec![0, 0, 0, 5], // Piece index 5
        };

        let bytes = msg.to_bytes();
        assert_eq!(
            bytes,
            vec![
                0, 0, 0, 5, // length = 5
                4, // MessageId::Have
                0, 0, 0, 5 // payload
            ]
        );

        let parsed = Message::from_bytes(&bytes).unwrap();
        assert_eq!(parsed, msg);
    }

    #[test]
    fn test_serialize_keep_alive() {
        let msg = Message {
            id: None,
            payload: vec![],
        };

        let bytes = msg.to_bytes();
        assert_eq!(bytes, vec![0, 0, 0, 0]);

        let parsed = Message::from_bytes(&bytes).unwrap();
        assert_eq!(parsed, msg);
    }

    mod message_parsing_tests {
        use super::*;
        #[test]
        fn test_parse_choke_message() {
            // 4-byte length = 1, id = 0 (Choke)
            let data = [0, 0, 0, 1, 0];
            let msg = Message::from_bytes(&data).unwrap();
            assert_eq!(msg.id, Some(MessageId::Choke));
            assert!(msg.payload.is_empty());
        }

        #[test]
        fn test_parse_bitfield_message_with_payload() {
            // 4-byte length = 3, id = 5 (Bitfield), payload = [0b10101010, 0b11000000]
            let data = [0, 0, 0, 3, 5, 0b_1010_1010, 0b_1100_0000];
            let msg = Message::from_bytes(&data).unwrap();
            assert_eq!(msg.id, Some(MessageId::Bitfield));
            assert_eq!(msg.payload, vec![0b_1010_1010, 0b_1100_0000]);
        }

        #[test]
        fn test_parse_have_message_with_payload() {
            // 4-byte length = 5, id = 4 (Have), payload = [0, 0, 0, 12]
            let data = [0, 0, 0, 5, 4, 0, 0, 0, 12];
            let msg = Message::from_bytes(&data).unwrap();
            assert_eq!(msg.id, Some(MessageId::Have));
            assert_eq!(msg.payload, vec![0, 0, 0, 12]);
        }
    }

    #[test]
    fn test_invalid_message_id() {
        let bytes = vec![0, 0, 0, 1, 99]; // 99 is invalid
        let result = Message::from_bytes(&bytes);
        match result {
            Err(MessageError::InvalidMessageId(99)) => {}
            _ => panic!("Expected InvalidMessageId error"),
        }
    }

    #[test]
    fn test_too_short_data() {
        let result = Message::from_bytes(&[0, 0]); // too short
        assert!(matches!(result, Err(MessageError::InvalidLength)));
    }

    #[test]
    fn test_incomplete_data() {
        let bytes = vec![0, 0, 0, 5, 4]; // length 5, but only 1 byte after header
        let result = Message::from_bytes(&bytes);
        assert!(matches!(result, Err(MessageError::IncompleteData)));
    }
}
