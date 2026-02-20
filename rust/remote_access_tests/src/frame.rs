use anyhow::{bail, ensure, Result};

/// Operation code for ws-protocol byte stream framing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum OpCode {
    /// The frame contains a JSON message.
    Text = 1,
    /// The frame contains a binary message.
    Binary = 2,
}

impl OpCode {
    fn from_u8(value: u8) -> Result<Self> {
        match value {
            1 => Ok(Self::Text),
            2 => Ok(Self::Binary),
            other => bail!("unknown opcode: {other}"),
        }
    }
}

/// A parsed frame from the ws-protocol byte stream.
#[derive(Debug)]
pub struct Frame {
    pub op_code: OpCode,
    pub payload: Vec<u8>,
}

/// Header size: 1 byte opcode + 4 byte LE u32 length.
pub const HEADER_SIZE: usize = 5;

/// Frames a binary protocol message for sending over the ws-protocol byte stream.
///
/// `inner` is the encoded protocol message (opcode + payload, as produced by
/// `BinaryMessage::to_bytes()`). The result is wrapped in the ws-protocol frame
/// format: `[Binary opcode (2)] [length: u32 LE] [inner]`.
pub fn frame_binary_message(inner: &[u8]) -> Vec<u8> {
    let len = inner.len() as u32;
    let mut buf = Vec::with_capacity(HEADER_SIZE + inner.len());
    buf.push(OpCode::Binary as u8);
    buf.extend_from_slice(&len.to_le_bytes());
    buf.extend_from_slice(inner);
    buf
}

/// Attempts to parse a single frame from the accumulated buffer.
///
/// Returns `Some((frame, bytes_consumed))` if a complete frame is available,
/// or `None` if more data is needed.
pub fn try_parse_frame(data: &[u8]) -> Result<Option<(Frame, usize)>> {
    if data.len() < HEADER_SIZE {
        return Ok(None);
    }
    let op_code = OpCode::from_u8(data[0])?;
    let length = u32::from_le_bytes([data[1], data[2], data[3], data[4]]) as usize;
    let total = HEADER_SIZE + length;
    if data.len() < total {
        return Ok(None);
    }
    ensure!(length > 0, "empty frame payload");
    let payload = data[HEADER_SIZE..total].to_vec();
    Ok(Some((Frame { op_code, payload }, total)))
}
