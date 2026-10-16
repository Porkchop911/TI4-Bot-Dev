//! Fixed memory for everything that crosses the wasm boundary.
//!
//! Each buffer is one static block of a fixed size. It is never grown, never freed, and its
//! address never changes, so the host reads a result at the same pointer after every call. A value
//! that does not fit is an error that names the constant to raise: the limits are meant to be hit
//! while they are cheap to change, not hidden behind a reallocation.

use std::fmt;
use std::io;
use std::sync::Mutex;

/// The largest result of an export, in bytes. A full game state is about 40 KiB (2026-10-09).
pub const RESPONSE_CAPACITY: usize = 256 * 1024;
/// The largest update for the host, in bytes: the view of a seat and the choice it is asked.
/// Early in a game of eight an update is about 40 KiB (2026-10-09).
pub const UPDATE_CAPACITY: usize = 512 * 1024;

/// The largest request of the host, in bytes: the script of a draft. A step is about 150 bytes.
pub const REQUEST_CAPACITY: usize = 64 * 1024;
/// The largest outcome of a draft, in bytes: two updates, where the draft stopped and where
/// its movement began.
pub const DRAFT_CAPACITY: usize = 1024 * 1024;

pub static RESPONSE: Mutex<Buffer<RESPONSE_CAPACITY>> = Mutex::new(Buffer::new());
pub static UPDATE: Mutex<Buffer<UPDATE_CAPACITY>> = Mutex::new(Buffer::new());
pub static REQUEST: Mutex<Buffer<REQUEST_CAPACITY>> = Mutex::new(Buffer::new());
pub static DRAFT: Mutex<Buffer<DRAFT_CAPACITY>> = Mutex::new(Buffer::new());

pub struct Buffer<const N: usize> {
    bytes: [u8; N],
    len: usize,
}

impl<const N: usize> Buffer<N> {
    const fn new() -> Self {
        Self {
            bytes: [0; N],
            len: 0,
        }
    }

    pub fn clear(&mut self) {
        self.len = 0;
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.len
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    #[must_use]
    pub fn as_ptr(&self) -> *const u8 {
        self.bytes.as_ptr()
    }

    /// Where the host writes a request, before it tells its length with [`Buffer::filled`].
    #[must_use]
    pub fn as_mut_ptr(&mut self) -> *mut u8 {
        self.bytes.as_mut_ptr()
    }

    /// The host wrote `len` bytes at [`Buffer::as_mut_ptr`]. False when that is more than fits.
    pub fn filled(&mut self, len: usize) -> bool {
        self.len = len.min(N);
        len <= N
    }

    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes[..self.len]
    }
}

/// All or nothing: a value is never cut short, because half a JSON document reads as a bug in the
/// host's parser rather than as a full buffer.
impl<const N: usize> io::Write for Buffer<N> {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        let end = self.len + data.len();
        if end > N {
            return Err(io::ErrorKind::StorageFull.into());
        }
        self.bytes[self.len..end].copy_from_slice(data);
        self.len = end;
        Ok(data.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// For error messages only: text that does not fit is cut, so that a failure can always be told.
impl<const N: usize> fmt::Write for Buffer<N> {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        let mut take = text.len().min(N - self.len);
        while !text.is_char_boundary(take) {
            take -= 1;
        }
        self.bytes[self.len..self.len + take].copy_from_slice(&text.as_bytes()[..take]);
        self.len += take;
        Ok(())
    }
}

/// Measures a value without storing it, to say how large a buffer would have had to be.
pub struct Counter(pub usize);

impl io::Write for Counter {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        self.0 += data.len();
        Ok(data.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// A value was larger than its buffer.
#[derive(Debug)]
pub struct TooLarge {
    pub what: &'static str,
    pub needed: usize,
    pub capacity: usize,
    pub constant: &'static str,
}

impl fmt::Display for TooLarge {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "ti4-wasm buffer full: {} is {} bytes, but {} is {} bytes. Raise {} in \
             crates/ti4-wasm/src/buffer.rs and rebuild, or make {} smaller.",
            self.what, self.needed, self.constant, self.capacity, self.constant, self.what
        )
    }
}

/// Replace the buffer's content with `value` as JSON.
///
/// # Errors
/// [`TooLarge`] with the size the value needs. The buffer is left empty.
pub fn store<const N: usize>(
    buffer: &mut Buffer<N>,
    what: &'static str,
    constant: &'static str,
    value: &impl serde::Serialize,
) -> Result<usize, TooLarge> {
    // Through `dyn Write`, so that storing and measuring share one copy of every type's
    // serializer. One copy per writer type cost 39 KB of wasm.
    fn write(writer: &mut dyn io::Write, value: &impl serde::Serialize) -> bool {
        serde_json::to_writer(writer, value).is_ok()
    }
    buffer.clear();
    if write(buffer, value) {
        return Ok(buffer.len());
    }
    buffer.clear();
    let mut counter = Counter(0);
    // The engine's types serialize infallibly; the only failure above is a full buffer.
    write(&mut counter, value);
    Err(TooLarge {
        what,
        needed: counter.0,
        capacity: N,
        constant,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_value_that_fits_is_stored_whole() {
        let mut buffer = Buffer::<16>::new();
        assert_eq!(
            store(&mut buffer, "the value", "CAPACITY", &[1, 2, 3]).unwrap(),
            7
        );
        assert_eq!(buffer.as_bytes(), b"[1,2,3]");
    }

    #[test]
    fn a_value_that_does_not_fit_names_its_size_and_the_constant() {
        let mut buffer = Buffer::<16>::new();
        let error = store(&mut buffer, "the value", "CAPACITY", &"0123456789abcdef").unwrap_err();
        assert!(buffer.is_empty());
        assert_eq!((error.needed, error.capacity), (18, 16));
        assert_eq!(
            error.to_string(),
            "ti4-wasm buffer full: the value is 18 bytes, but CAPACITY is 16 bytes. Raise \
             CAPACITY in crates/ti4-wasm/src/buffer.rs and rebuild, or make the value smaller."
        );
    }

    #[test]
    fn an_error_text_is_cut_at_a_character_and_never_fails() {
        let mut buffer = Buffer::<4>::new();
        fmt::Write::write_str(&mut buffer, "aaaé").unwrap();
        assert_eq!(buffer.as_bytes(), b"aaa");
    }
}
