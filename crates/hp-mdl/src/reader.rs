use crate::error::{FileKind, MdlError};

const MAX_CSTR: usize = 1024;

#[derive(Clone, Copy)]
pub(crate) struct Reader<'a> {
    data: &'a [u8],
    file: FileKind,
}

pub(crate) type Result<T> = std::result::Result<T, MdlError>;

impl<'a> Reader<'a> {
    pub(crate) fn new(data: &'a [u8], file: FileKind) -> Self {
        Self { data, file }
    }

    pub(crate) fn oob(&self, what: &'static str, offset: i64, size: u64) -> MdlError {
        MdlError::OutOfBounds { file: self.file, what, offset, size, file_len: self.data.len() }
    }

    pub(crate) fn invalid(&self, what: &'static str, value: i64) -> MdlError {
        MdlError::InvalidValue { file: self.file, what, value }
    }

    #[inline]
    pub(crate) fn bytes(&self, offset: usize, size: usize, what: &'static str) -> Result<&'a [u8]> {
        offset
            .checked_add(size)
            .and_then(|end| self.data.get(offset..end))
            .ok_or_else(|| self.oob(what, offset as i64, size as u64))
    }

    #[inline]
    pub(crate) fn check(&self, offset: usize, size: usize, what: &'static str) -> Result<()> {
        self.bytes(offset, size, what).map(|_| ())
    }

    #[inline]
    fn array<const N: usize>(&self, offset: usize) -> Result<[u8; N]> {
        let bytes = self.bytes(offset, N, "field")?;
        let mut out = [0u8; N];
        out.copy_from_slice(bytes);
        Ok(out)
    }

    #[inline]
    pub(crate) fn u8(&self, offset: usize) -> Result<u8> {
        self.data.get(offset).copied().ok_or_else(|| self.oob("field", offset as i64, 1))
    }

    #[inline]
    pub(crate) fn i16(&self, offset: usize) -> Result<i16> {
        self.array::<2>(offset).map(i16::from_le_bytes)
    }

    #[inline]
    pub(crate) fn u16(&self, offset: usize) -> Result<u16> {
        self.array::<2>(offset).map(u16::from_le_bytes)
    }

    #[inline]
    pub(crate) fn i32(&self, offset: usize) -> Result<i32> {
        self.array::<4>(offset).map(i32::from_le_bytes)
    }

    #[inline]
    pub(crate) fn u32(&self, offset: usize) -> Result<u32> {
        self.array::<4>(offset).map(u32::from_le_bytes)
    }

    #[inline]
    pub(crate) fn f32(&self, offset: usize) -> Result<f32> {
        self.array::<4>(offset).map(f32::from_le_bytes)
    }

    #[inline]
    pub(crate) fn floats<const N: usize>(&self, offset: usize) -> Result<[f32; N]> {
        let bytes = self.bytes(offset, N * 4, "field")?;
        let mut out = [0f32; N];
        for (dst, src) in out.iter_mut().zip(bytes.chunks_exact(4)) {
            *dst = f32::from_le_bytes([src[0], src[1], src[2], src[3]]);
        }
        Ok(out)
    }

    pub(crate) fn cstr(&self, offset: usize, what: &'static str) -> Result<String> {
        let tail = self.data.get(offset..).ok_or_else(|| self.oob(what, offset as i64, 1))?;
        let window = &tail[..tail.len().min(MAX_CSTR)];
        let end =
            window.iter().position(|&b| b == 0).ok_or_else(|| self.invalid("unterminated string", offset as i64))?;
        Ok(String::from_utf8_lossy(&window[..end]).into_owned())
    }

    pub(crate) fn fixed_str(&self, offset: usize, len: usize, what: &'static str) -> Result<String> {
        let bytes = self.bytes(offset, len, what)?;
        let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
        Ok(String::from_utf8_lossy(&bytes[..end]).into_owned())
    }

    #[inline]
    pub(crate) fn rel(&self, base: usize, rel: i32, what: &'static str) -> Result<usize> {
        let target = base as i64 + i64::from(rel);
        if target < 0 || target as u64 > self.data.len() as u64 {
            return Err(self.oob(what, target, 0));
        }
        Ok(target as usize)
    }

    #[inline]
    pub(crate) fn table(&self, offset: usize, count: i32, stride: usize, what: &'static str) -> Result<usize> {
        let count = usize::try_from(count).map_err(|_| self.invalid(what, i64::from(count)))?;
        let size = count.checked_mul(stride).ok_or_else(|| self.invalid(what, count as i64))?;
        self.check(offset, size, what)?;
        Ok(count)
    }

    #[inline]
    pub(crate) fn rel_table(
        &self,
        base: usize,
        rel: i32,
        count: i32,
        stride: usize,
        what: &'static str,
    ) -> Result<(usize, usize)> {
        if count == 0 {
            return Ok((0, 0));
        }
        let start = self.rel(base, rel, what)?;
        let count = self.table(start, count, stride, what)?;
        Ok((start, count))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_and_bounds() {
        let data = [1u8, 0, 0, 0, b'a', b'b', 0, 0xff];
        let r = Reader::new(&data, FileKind::Mdl);
        assert_eq!(r.i32(0).ok(), Some(1));
        assert_eq!(r.cstr(4, "s").ok().as_deref(), Some("ab"));
        assert!(r.i32(5).is_err());
        assert!(r.cstr(7, "s").is_err());
        assert!(r.cstr(100, "s").is_err());
        assert!(r.rel(4, -5, "x").is_err());
        assert_eq!(r.rel(4, -4, "x").ok(), Some(0));
        assert!(r.table(0, -1, 4, "t").is_err());
        assert!(r.table(0, i32::MAX, usize::MAX / 2, "t").is_err());
        assert_eq!(r.table(4, 1, 4, "t").ok(), Some(1));
        assert_eq!(r.rel_table(0, 1_000_000, 0, 4, "t").ok(), Some((0, 0)));
        assert_eq!(r.fixed_str(4, 4, "f").ok().as_deref(), Some("ab"));
    }
}
