use std::fmt;

pub const HEADER: usize = 8;
pub const MAX_PAYLOAD: usize = 64 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum Opcode {
    Handshake = 0,
    Frame = 1,
    Close = 2,
    Ping = 3,
    Pong = 4,
}

impl Opcode {
    pub fn from_u32(op: u32) -> Option<Opcode> {
        Some(match op {
            0 => Opcode::Handshake,
            1 => Opcode::Frame,
            2 => Opcode::Close,
            3 => Opcode::Ping,
            4 => Opcode::Pong,
            _ => return None,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    pub op: Opcode,
    pub payload: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FrameError {
    UnknownOpcode(u32),
    TooLong(usize),
    NotUtf8,
}

impl fmt::Display for FrameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FrameError::UnknownOpcode(op) => write!(f, "unknown opcode {op}"),
            FrameError::TooLong(n) => write!(f, "frame of {n} bytes is too long"),
            FrameError::NotUtf8 => f.write_str("frame payload is not UTF-8"),
        }
    }
}

impl std::error::Error for FrameError {}

pub fn encode(op: Opcode, payload: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(HEADER + payload.len());
    out.extend_from_slice(&(op as u32).to_le_bytes());
    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    out.extend_from_slice(payload.as_bytes());
    out
}

pub fn decode(buf: &[u8]) -> Result<Option<(Frame, usize)>, FrameError> {
    if buf.len() < HEADER {
        return Ok(None);
    }
    let op = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
    let len = u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]) as usize;
    let op = Opcode::from_u32(op).ok_or(FrameError::UnknownOpcode(op))?;
    if len > MAX_PAYLOAD {
        return Err(FrameError::TooLong(len));
    }
    let Some(body) = buf.get(HEADER..HEADER + len) else {
        return Ok(None);
    };
    let payload = std::str::from_utf8(body).map_err(|_| FrameError::NotUtf8)?.to_owned();
    Ok(Some((Frame { op, payload }, HEADER + len)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let bytes = encode(Opcode::Frame, "{\"cmd\":\"SET_ACTIVITY\"}");
        assert_eq!(&bytes[..4], &[1, 0, 0, 0]);
        assert_eq!(&bytes[4..8], &[22, 0, 0, 0]);
        let (frame, used) = decode(&bytes).unwrap().unwrap();
        assert_eq!(used, bytes.len());
        assert_eq!(frame, Frame { op: Opcode::Frame, payload: "{\"cmd\":\"SET_ACTIVITY\"}".into() });
    }

    #[test]
    fn partial_and_chained_frames() {
        let mut bytes = encode(Opcode::Handshake, "{\"v\":1}");
        bytes.extend(encode(Opcode::Close, ""));
        for cut in 0..15 {
            assert_eq!(decode(&bytes[..cut]).unwrap(), None, "{cut}");
        }
        let (first, used) = decode(&bytes).unwrap().unwrap();
        assert_eq!(first.op, Opcode::Handshake);
        let (second, rest) = decode(&bytes[used..]).unwrap().unwrap();
        assert_eq!((second.op, second.payload.as_str(), used + rest), (Opcode::Close, "", bytes.len()));
    }

    #[test]
    fn bad_frames() {
        let mut bytes = encode(Opcode::Frame, "x");
        bytes[0] = 9;
        assert_eq!(decode(&bytes), Err(FrameError::UnknownOpcode(9)));
        let mut long = encode(Opcode::Frame, "");
        long[4..8].copy_from_slice(&(MAX_PAYLOAD as u32 + 1).to_le_bytes());
        assert_eq!(decode(&long), Err(FrameError::TooLong(MAX_PAYLOAD + 1)));
        let mut bad = encode(Opcode::Frame, "ab");
        bad[8] = 0xff;
        assert_eq!(decode(&bad), Err(FrameError::NotUtf8));
    }
}
