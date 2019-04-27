use crate::schema::*;

use rustc::hir::def_id::{DefId, DefIndex, DefIndexAddressSpace};
use rustc_serialize::opaque::Encoder;
use std::u32;
use log::debug;

/// Helper trait, for encoding to, and decoding from, a fixed number of bytes.
pub trait FixedSizeEncoding {
    const BYTE_LEN: usize;

    // FIXME(eddyb) convert to and from `[u8; Self::BYTE_LEN]` instead,
    // once that starts being allowed by the compiler (i.e. lazy normalization).
    fn from_bytes(b: &[u8]) -> Self;
    fn write_to_bytes(self, b: &mut [u8]);

    // FIXME(eddyb) make these generic functions, or at least defaults here.
    // (same problem as above, needs `[u8; Self::BYTE_LEN]`)
    // For now, a macro (`fixed_size_encoding_byte_len_and_defaults`) is used.
    fn read_from_bytes_at(b: &[u8], i: usize) -> Self;
    fn write_to_bytes_at(self, b: &mut [u8], i: usize);
}

// HACK(eddyb) this shouldn't be needed (see comments on the methods above).
macro_rules! fixed_size_encoding_byte_len_and_defaults {
    ($byte_len:expr) => {
        const BYTE_LEN: usize = $byte_len;
        fn read_from_bytes_at(b: &[u8], i: usize) -> Self {
            const BYTE_LEN: usize = $byte_len;
            // HACK(eddyb) ideally this would be done with fully safe code,
            // but slicing `[u8]` with `i * N..` is optimized worse, due to the
            // possibility of `i * N` overflowing, than indexing `[[u8; N]]`.
            let b = unsafe {
                std::slice::from_raw_parts(
                    b.as_ptr() as *const [u8; BYTE_LEN],
                    b.len() / BYTE_LEN,
                )
            };
            Self::from_bytes(&b[i])
        }
        fn write_to_bytes_at(self, b: &mut [u8], i: usize) {
            const BYTE_LEN: usize = $byte_len;
            // HACK(eddyb) ideally this would be done with fully safe code,
            // see similar comment in `read_from_bytes_at` for why it can't yet.
            let b = unsafe {
                std::slice::from_raw_parts_mut(
                    b.as_mut_ptr() as *mut [u8; BYTE_LEN],
                    b.len() / BYTE_LEN,
                )
            };
            self.write_to_bytes(&mut b[i]);
        }
    }
}

impl FixedSizeEncoding for u32 {
    fixed_size_encoding_byte_len_and_defaults!(4);

    fn from_bytes(b: &[u8]) -> Self {
        let mut bytes = [0; Self::BYTE_LEN];
        bytes.copy_from_slice(&b[..Self::BYTE_LEN]);
        Self::from_le_bytes(bytes)
    }

    fn write_to_bytes(self, b: &mut [u8]) {
        b[..Self::BYTE_LEN].copy_from_slice(&self.to_le_bytes());
    }
}

/// While we are generating the metadata, we also track the position
/// of each DefIndex. It is not required that all definitions appear
/// in the metadata, nor that they are serialized in order, and
/// therefore we first allocate the vector here and fill it with
/// `u32::MAX`. Whenever an index is visited, we fill in the
/// appropriate spot by calling `record_position`. We should never
/// visit the same index twice.
pub struct Index {
    positions: [Vec<u8>; 2]
}

impl Index {
    pub fn new((max_index_lo, max_index_hi): (usize, usize)) -> Index {
        Index {
            positions: [vec![0xff; max_index_lo * 4],
                        vec![0xff; max_index_hi * 4]],
        }
    }

    pub fn record(&mut self, def_id: DefId, entry: Lazy<Entry<'_>>) {
        assert!(def_id.is_local());
        self.record_index(def_id.index, entry);
    }

    pub fn record_index(&mut self, item: DefIndex, entry: Lazy<Entry<'_>>) {
        assert!(entry.position < (u32::MAX as usize));
        let position = entry.position as u32;
        let space_index = item.address_space().index();
        let array_index = item.as_array_index();

        let positions = &mut self.positions[space_index];
        assert!(u32::read_from_bytes_at(positions, array_index) == u32::MAX,
                "recorded position for item {:?} twice, first at {:?} and now at {:?}",
                item,
                u32::read_from_bytes_at(positions, array_index),
                position);

        position.write_to_bytes_at(positions, array_index)
    }

    pub fn write_index(&self, buf: &mut Encoder) -> LazySeq<Index> {
        let pos = buf.position();

        // First we write the length of the lower range ...
        buf.emit_raw_bytes(&(self.positions[0].len() as u32 / 4).to_le_bytes());
        // ... then the values in the lower range ...
        buf.emit_raw_bytes(&self.positions[0]);
        // ... then the values in the higher range.
        buf.emit_raw_bytes(&self.positions[1]);
        LazySeq::with_position_and_length(pos as usize,
            (self.positions[0].len() + self.positions[1].len()) / 4 + 1)
    }
}

impl<'tcx> LazySeq<Index> {
    /// Given the metadata, extract out the offset of a particular
    /// DefIndex (if any).
    #[inline(never)]
    pub fn lookup(&self, bytes: &[u8], def_index: DefIndex) -> Option<Lazy<Entry<'tcx>>> {
        let bytes = &bytes[self.position..];
        debug!("Index::lookup: index={:?} len={:?}",
               def_index,
               self.len);

        let i = def_index.as_array_index() + match def_index.address_space() {
            DefIndexAddressSpace::Low => 0,
            DefIndexAddressSpace::High => {
                // This is a DefIndex in the higher range, so find out where
                // that starts:
                u32::read_from_bytes_at(bytes, 0) as usize
            }
        };

        let position = u32::read_from_bytes_at(bytes, 1 + i);
        if position == u32::MAX {
            debug!("Index::lookup: position=u32::MAX");
            None
        } else {
            debug!("Index::lookup: position={:?}", position);
            Some(Lazy::with_position(position as usize))
        }
    }
}
