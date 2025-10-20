#![allow(dead_code)]

use std::convert::TryFrom;
use std::fmt;
use std::io::{Read, Write};

#[derive(Debug)]
pub enum MessageError {
    InvalidLength,
    IncompleteData,
    InvalidMessageId(u8),
    IoError(std::io::Error),
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
        let payload = data[5..].to_vec();

        Ok(Message {
            id: Some(id),
            payload,
        })
    }

    pub fn read_from_stream<R: Read>(reader: &mut R) -> Result<Self, MessageError> {
        let mut len_buf = [0u8; 4];
        reader.read_exact(&mut len_buf)?;
        let msg_len = u32::from_be_bytes(len_buf) as usize;

        if msg_len == 0 {
            return Ok(Message {
                id: None,
                payload: Vec::new(),
            });
        }

        let mut msg_buf = vec![0u8; msg_len];
        reader.read_exact(&mut msg_buf)?;

        let mut full_msg = len_buf.to_vec();
        full_msg.extend_from_slice(&msg_buf);

        Self::from_bytes(&full_msg)
    }

    pub fn write_to_stream<W: Write>(&self, writer: &mut W) -> Result<(), MessageError> {
        let bytes = self.to_bytes();
        writer.write_all(&bytes)?;
        Ok(())
    }
}

// --- Tests ---

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

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

    #[test]
    fn test_read_from_stream_bitfield() {
        let payload = vec![0b10101010, 0b11000000];
        let msg: Message = Message {
            id: Some(MessageId::Bitfield),
            payload: payload.clone(),
        };
        let buffer = Message::to_bytes(&msg);
        let mut cursor = Cursor::new(buffer);

        let message = Message::read_from_stream(&mut cursor).expect("should parse correctly");

        assert_eq!(message.id, Some(MessageId::Bitfield));
        assert_eq!(message.payload, payload);
    }

    #[test]
    fn test_write_keep_alive_message() {
        let msg: Message = Message {
            id: None,
            payload: vec![],
        };

        let mut buf = Vec::new();
        msg.write_to_stream(&mut buf).unwrap();

        assert_eq!(buf, vec![0, 0, 0, 0]);
    }
}
