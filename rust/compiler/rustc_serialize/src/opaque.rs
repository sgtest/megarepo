use crate::leb128;
use crate::serialize::{Decodable, Decoder, Encodable, Encoder};
use std::fs::File;
use std::io::{self, Write};
use std::marker::PhantomData;
use std::mem::MaybeUninit;
use std::ops::Range;
use std::path::Path;
use std::ptr;

// -----------------------------------------------------------------------------
// Encoder
// -----------------------------------------------------------------------------

pub type FileEncodeResult = Result<usize, io::Error>;

/// The size of the buffer in `FileEncoder`.
const BUF_SIZE: usize = 8192;

/// `FileEncoder` encodes data to file via fixed-size buffer.
///
/// There used to be a `MemEncoder` type that encoded all the data into a
/// `Vec`. `FileEncoder` is better because its memory use is determined by the
/// size of the buffer, rather than the full length of the encoded data, and
/// because it doesn't need to reallocate memory along the way.
pub struct FileEncoder {
    /// The input buffer. For adequate performance, we need more control over
    /// buffering than `BufWriter` offers. If `BufWriter` ever offers a raw
    /// buffer access API, we can use it, and remove `buf` and `buffered`.
    buf: Box<[MaybeUninit<u8>]>,
    buffered: usize,
    flushed: usize,
    file: File,
    // This is used to implement delayed error handling, as described in the
    // comment on `trait Encoder`.
    res: Result<(), io::Error>,
}

impl FileEncoder {
    pub fn new<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        // Create the file for reading and writing, because some encoders do both
        // (e.g. the metadata encoder when -Zmeta-stats is enabled)
        let file = File::options().read(true).write(true).create(true).truncate(true).open(path)?;

        Ok(FileEncoder {
            buf: Box::new_uninit_slice(BUF_SIZE),
            buffered: 0,
            flushed: 0,
            file,
            res: Ok(()),
        })
    }

    #[inline]
    pub fn position(&self) -> usize {
        // Tracking position this way instead of having a `self.position` field
        // means that we don't have to update the position on every write call.
        self.flushed + self.buffered
    }

    pub fn flush(&mut self) {
        // This is basically a copy of `BufWriter::flush`. If `BufWriter` ever
        // offers a raw buffer access API, we can use it, and remove this.

        /// Helper struct to ensure the buffer is updated after all the writes
        /// are complete. It tracks the number of written bytes and drains them
        /// all from the front of the buffer when dropped.
        struct BufGuard<'a> {
            buffer: &'a mut [u8],
            encoder_buffered: &'a mut usize,
            encoder_flushed: &'a mut usize,
            flushed: usize,
        }

        impl<'a> BufGuard<'a> {
            fn new(
                buffer: &'a mut [u8],
                encoder_buffered: &'a mut usize,
                encoder_flushed: &'a mut usize,
            ) -> Self {
                assert_eq!(buffer.len(), *encoder_buffered);
                Self { buffer, encoder_buffered, encoder_flushed, flushed: 0 }
            }

            /// The unwritten part of the buffer
            fn remaining(&self) -> &[u8] {
                &self.buffer[self.flushed..]
            }

            /// Flag some bytes as removed from the front of the buffer
            fn consume(&mut self, amt: usize) {
                self.flushed += amt;
            }

            /// true if all of the bytes have been written
            fn done(&self) -> bool {
                self.flushed >= *self.encoder_buffered
            }
        }

        impl Drop for BufGuard<'_> {
            fn drop(&mut self) {
                if self.flushed > 0 {
                    if self.done() {
                        *self.encoder_flushed += *self.encoder_buffered;
                        *self.encoder_buffered = 0;
                    } else {
                        self.buffer.copy_within(self.flushed.., 0);
                        *self.encoder_flushed += self.flushed;
                        *self.encoder_buffered -= self.flushed;
                    }
                }
            }
        }

        // If we've already had an error, do nothing. It'll get reported after
        // `finish` is called.
        if self.res.is_err() {
            return;
        }

        let mut guard = BufGuard::new(
            unsafe { MaybeUninit::slice_assume_init_mut(&mut self.buf[..self.buffered]) },
            &mut self.buffered,
            &mut self.flushed,
        );

        while !guard.done() {
            match self.file.write(guard.remaining()) {
                Ok(0) => {
                    self.res = Err(io::Error::new(
                        io::ErrorKind::WriteZero,
                        "failed to write the buffered data",
                    ));
                    return;
                }
                Ok(n) => guard.consume(n),
                Err(ref e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => {
                    self.res = Err(e);
                    return;
                }
            }
        }
    }

    pub fn file(&self) -> &File {
        &self.file
    }

    #[inline]
    fn write_one(&mut self, value: u8) {
        let mut buffered = self.buffered;

        if std::intrinsics::unlikely(buffered + 1 > BUF_SIZE) {
            self.flush();
            buffered = 0;
        }

        // SAFETY: The above check and `flush` ensures that there is enough
        // room to write the input to the buffer.
        unsafe {
            *MaybeUninit::slice_as_mut_ptr(&mut self.buf).add(buffered) = value;
        }

        self.buffered = buffered + 1;
    }

    #[inline]
    fn write_all(&mut self, buf: &[u8]) {
        let buf_len = buf.len();

        if std::intrinsics::likely(buf_len <= BUF_SIZE) {
            let mut buffered = self.buffered;

            if std::intrinsics::unlikely(buffered + buf_len > BUF_SIZE) {
                self.flush();
                buffered = 0;
            }

            // SAFETY: The above check and `flush` ensures that there is enough
            // room to write the input to the buffer.
            unsafe {
                let src = buf.as_ptr();
                let dst = MaybeUninit::slice_as_mut_ptr(&mut self.buf).add(buffered);
                ptr::copy_nonoverlapping(src, dst, buf_len);
            }

            self.buffered = buffered + buf_len;
        } else {
            self.write_all_unbuffered(buf);
        }
    }

    fn write_all_unbuffered(&mut self, mut buf: &[u8]) {
        // If we've already had an error, do nothing. It'll get reported after
        // `finish` is called.
        if self.res.is_err() {
            return;
        }

        if self.buffered > 0 {
            self.flush();
        }

        // This is basically a copy of `Write::write_all` but also updates our
        // `self.flushed`. It's necessary because `Write::write_all` does not
        // return the number of bytes written when an error is encountered, and
        // without that, we cannot accurately update `self.flushed` on error.
        while !buf.is_empty() {
            match self.file.write(buf) {
                Ok(0) => {
                    self.res = Err(io::Error::new(
                        io::ErrorKind::WriteZero,
                        "failed to write whole buffer",
                    ));
                    return;
                }
                Ok(n) => {
                    buf = &buf[n..];
                    self.flushed += n;
                }
                Err(ref e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => {
                    self.res = Err(e);
                    return;
                }
            }
        }
    }

    pub fn finish(mut self) -> Result<usize, io::Error> {
        self.flush();

        let res = std::mem::replace(&mut self.res, Ok(()));
        res.map(|()| self.position())
    }
}

impl Drop for FileEncoder {
    fn drop(&mut self) {
        // Likely to be a no-op, because `finish` should have been called and
        // it also flushes. But do it just in case.
        let _result = self.flush();
    }
}

macro_rules! write_leb128 {
    ($this_fn:ident, $int_ty:ty, $write_leb_fn:ident) => {
        #[inline]
        fn $this_fn(&mut self, v: $int_ty) {
            const MAX_ENCODED_LEN: usize = $crate::leb128::max_leb128_len::<$int_ty>();

            let mut buffered = self.buffered;

            // This can't overflow because BUF_SIZE and MAX_ENCODED_LEN are both
            // quite small.
            if std::intrinsics::unlikely(buffered + MAX_ENCODED_LEN > BUF_SIZE) {
                self.flush();
                buffered = 0;
            }

            // SAFETY: The above check and flush ensures that there is enough
            // room to write the encoded value to the buffer.
            let buf = unsafe {
                &mut *(self.buf.as_mut_ptr().add(buffered)
                    as *mut [MaybeUninit<u8>; MAX_ENCODED_LEN])
            };

            let encoded = leb128::$write_leb_fn(buf, v);
            self.buffered = buffered + encoded.len();
        }
    };
}

impl Encoder for FileEncoder {
    write_leb128!(emit_usize, usize, write_usize_leb128);
    write_leb128!(emit_u128, u128, write_u128_leb128);
    write_leb128!(emit_u64, u64, write_u64_leb128);
    write_leb128!(emit_u32, u32, write_u32_leb128);

    #[inline]
    fn emit_u16(&mut self, v: u16) {
        self.write_all(&v.to_le_bytes());
    }

    #[inline]
    fn emit_u8(&mut self, v: u8) {
        self.write_one(v);
    }

    write_leb128!(emit_isize, isize, write_isize_leb128);
    write_leb128!(emit_i128, i128, write_i128_leb128);
    write_leb128!(emit_i64, i64, write_i64_leb128);
    write_leb128!(emit_i32, i32, write_i32_leb128);

    #[inline]
    fn emit_i16(&mut self, v: i16) {
        self.write_all(&v.to_le_bytes());
    }

    #[inline]
    fn emit_raw_bytes(&mut self, s: &[u8]) {
        self.write_all(s);
    }
}

// -----------------------------------------------------------------------------
// Decoder
// -----------------------------------------------------------------------------

// Conceptually, `MemDecoder` wraps a `&[u8]` with a cursor into it that is always valid.
// This is implemented with three pointers, two which represent the original slice and a
// third that is our cursor.
// It is an invariant of this type that start <= current <= end.
// Additionally, the implementation of this type never modifies start and end.
pub struct MemDecoder<'a> {
    start: *const u8,
    current: *const u8,
    end: *const u8,
    _marker: PhantomData<&'a u8>,
}

impl<'a> MemDecoder<'a> {
    #[inline]
    pub fn new(data: &'a [u8], position: usize) -> MemDecoder<'a> {
        let Range { start, end } = data.as_ptr_range();
        MemDecoder { start, current: data[position..].as_ptr(), end, _marker: PhantomData }
    }

    #[inline]
    pub fn data(&self) -> &'a [u8] {
        // SAFETY: This recovers the original slice, only using members we never modify.
        unsafe { std::slice::from_raw_parts(self.start, self.len()) }
    }

    #[inline]
    pub fn len(&self) -> usize {
        // SAFETY: This recovers the length of the original slice, only using members we never modify.
        unsafe { self.end.sub_ptr(self.start) }
    }

    #[inline]
    pub fn remaining(&self) -> usize {
        // SAFETY: This type guarantees current <= end.
        unsafe { self.end.sub_ptr(self.current) }
    }

    #[cold]
    #[inline(never)]
    fn decoder_exhausted() -> ! {
        panic!("MemDecoder exhausted")
    }

    #[inline]
    pub fn read_array<const N: usize>(&mut self) -> [u8; N] {
        self.read_raw_bytes(N).try_into().unwrap()
    }

    /// While we could manually expose manipulation of the decoder position,
    /// all current users of that method would need to reset the position later,
    /// incurring the bounds check of set_position twice.
    #[inline]
    pub fn with_position<F, T>(&mut self, pos: usize, func: F) -> T
    where
        F: Fn(&mut MemDecoder<'a>) -> T,
    {
        struct SetOnDrop<'a, 'guarded> {
            decoder: &'guarded mut MemDecoder<'a>,
            current: *const u8,
        }
        impl Drop for SetOnDrop<'_, '_> {
            fn drop(&mut self) {
                self.decoder.current = self.current;
            }
        }

        if pos >= self.len() {
            Self::decoder_exhausted();
        }
        let previous = self.current;
        // SAFETY: We just checked if this add is in-bounds above.
        unsafe {
            self.current = self.start.add(pos);
        }
        let guard = SetOnDrop { current: previous, decoder: self };
        func(guard.decoder)
    }
}

macro_rules! read_leb128 {
    ($this_fn:ident, $int_ty:ty, $read_leb_fn:ident) => {
        #[inline]
        fn $this_fn(&mut self) -> $int_ty {
            leb128::$read_leb_fn(self)
        }
    };
}

impl<'a> Decoder for MemDecoder<'a> {
    read_leb128!(read_usize, usize, read_usize_leb128);
    read_leb128!(read_u128, u128, read_u128_leb128);
    read_leb128!(read_u64, u64, read_u64_leb128);
    read_leb128!(read_u32, u32, read_u32_leb128);

    #[inline]
    fn read_u16(&mut self) -> u16 {
        u16::from_le_bytes(self.read_array())
    }

    #[inline]
    fn read_u8(&mut self) -> u8 {
        if self.current == self.end {
            Self::decoder_exhausted();
        }
        // SAFETY: This type guarantees current <= end, and we just checked current == end.
        unsafe {
            let byte = *self.current;
            self.current = self.current.add(1);
            byte
        }
    }

    read_leb128!(read_isize, isize, read_isize_leb128);
    read_leb128!(read_i128, i128, read_i128_leb128);
    read_leb128!(read_i64, i64, read_i64_leb128);
    read_leb128!(read_i32, i32, read_i32_leb128);

    #[inline]
    fn read_i16(&mut self) -> i16 {
        i16::from_le_bytes(self.read_array())
    }

    #[inline]
    fn read_raw_bytes(&mut self, bytes: usize) -> &'a [u8] {
        if bytes > self.remaining() {
            Self::decoder_exhausted();
        }
        // SAFETY: We just checked if this range is in-bounds above.
        unsafe {
            let slice = std::slice::from_raw_parts(self.current, bytes);
            self.current = self.current.add(bytes);
            slice
        }
    }

    #[inline]
    fn peek_byte(&self) -> u8 {
        if self.current == self.end {
            Self::decoder_exhausted();
        }
        // SAFETY: This type guarantees current is inbounds or one-past-the-end, which is end.
        // Since we just checked current == end, the current pointer must be inbounds.
        unsafe { *self.current }
    }

    #[inline]
    fn position(&self) -> usize {
        // SAFETY: This type guarantees start <= current
        unsafe { self.current.sub_ptr(self.start) }
    }
}

// Specializations for contiguous byte sequences follow. The default implementations for slices
// encode and decode each element individually. This isn't necessary for `u8` slices when using
// opaque encoders and decoders, because each `u8` is unchanged by encoding and decoding.
// Therefore, we can use more efficient implementations that process the entire sequence at once.

// Specialize encoding byte slices. This specialization also applies to encoding `Vec<u8>`s, etc.,
// since the default implementations call `encode` on their slices internally.
impl Encodable<FileEncoder> for [u8] {
    fn encode(&self, e: &mut FileEncoder) {
        Encoder::emit_usize(e, self.len());
        e.emit_raw_bytes(self);
    }
}

// Specialize decoding `Vec<u8>`. This specialization also applies to decoding `Box<[u8]>`s, etc.,
// since the default implementations call `decode` to produce a `Vec<u8>` internally.
impl<'a> Decodable<MemDecoder<'a>> for Vec<u8> {
    fn decode(d: &mut MemDecoder<'a>) -> Self {
        let len = Decoder::read_usize(d);
        d.read_raw_bytes(len).to_owned()
    }
}

/// An integer that will always encode to 8 bytes.
pub struct IntEncodedWithFixedSize(pub u64);

impl IntEncodedWithFixedSize {
    pub const ENCODED_SIZE: usize = 8;
}

impl Encodable<FileEncoder> for IntEncodedWithFixedSize {
    #[inline]
    fn encode(&self, e: &mut FileEncoder) {
        let _start_pos = e.position();
        e.emit_raw_bytes(&self.0.to_le_bytes());
        let _end_pos = e.position();
        debug_assert_eq!((_end_pos - _start_pos), IntEncodedWithFixedSize::ENCODED_SIZE);
    }
}

impl<'a> Decodable<MemDecoder<'a>> for IntEncodedWithFixedSize {
    #[inline]
    fn decode(decoder: &mut MemDecoder<'a>) -> IntEncodedWithFixedSize {
        let bytes = decoder.read_array::<{ IntEncodedWithFixedSize::ENCODED_SIZE }>();
        IntEncodedWithFixedSize(u64::from_le_bytes(bytes))
    }
}
