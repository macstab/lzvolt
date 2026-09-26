//! The C ABI: `lzvolt.h` on one side, this file on the other.
//!
//! Everything here is a thin shell over the Rust entry points. The rules it
//! follows, because a C caller cannot see them from the outside:
//!
//! - **Buffers belong to the caller.** Nothing here allocates on the caller's
//!   behalf and nothing has to be freed, so there is no ownership question and
//!   no `lzvolt_free`. The one exception is internal and invisible: a
//!   thread-local scratch buffer the compressor writes into, described at
//!   [`lzvolt_compress`](crate::ffi::lzvolt_compress).
//! - **Decompression writes straight into the caller's buffer.** No copy. That
//!   is not a detail — the decoder reads at over 13 GiB/s, so handing its
//!   output through a `memcpy` would be a fifth of the cost of the decode.
//! - **Every length is a `size_t`, every result an `int64_t`.** A result of
//!   zero or more is the number of bytes written; a negative result is one of
//!   the `LZVOLT_E_*` codes, and nothing was written.
//! - **Null is checked, not trusted.** A null pointer with a non-zero length
//!   is [`LZVOLT_E_NULL`](crate::ffi::LZVOLT_E_NULL), not a crash. A null pointer with a zero length is
//!   fine, because that is what an empty slice looks like in C.
//! - **Nothing unwinds across the boundary.** `extern "C"` aborts rather than
//!   unwinding, so a panic here would end the process instead of corrupting
//!   the caller's stack. None of these paths panics: the decoders return
//!   errors, and the only allocation is the scratch buffer.

use crate::format::{self, PackError};
use std::cell::RefCell;
use std::ffi::c_char;

/// Returned by [`lzvolt_compress`] when the input is not worth packing.
///
/// Not an error. It means "store these bytes as they are" — see the encoder
/// rules in `docs/FORMAT.md`. No valid stream is zero bytes long, so this
/// cannot collide with a real result.
pub const LZVOLT_NOT_COMPRESSIBLE: i64 = 0;

/// A pointer was null where a length said there would be bytes.
pub const LZVOLT_E_NULL: i64 = -1;
/// The stream ends in the middle of a field.
pub const LZVOLT_E_TRUNCATED: i64 = -2;
/// A backward reference points before the start of the output.
pub const LZVOLT_E_BAD_OFFSET: i64 = -3;
/// The stream does not produce the length it declares.
pub const LZVOLT_E_LENGTH_MISMATCH: i64 = -4;
/// The declared length is larger than any value may be.
pub const LZVOLT_E_TOO_LARGE: i64 = -5;
/// The header spends more bytes than its length needs.
pub const LZVOLT_E_NON_CANONICAL: i64 = -6;
/// The output buffer cannot hold the declared length.
pub const LZVOLT_E_OUTPUT_TOO_SMALL: i64 = -7;

fn code_for(e: PackError) -> i64 {
    match e {
        PackError::Truncated => LZVOLT_E_TRUNCATED,
        PackError::BadOffset => LZVOLT_E_BAD_OFFSET,
        PackError::LengthMismatch => LZVOLT_E_LENGTH_MISMATCH,
        PackError::TooLarge => LZVOLT_E_TOO_LARGE,
        PackError::NonCanonicalHeader => LZVOLT_E_NON_CANONICAL,
        PackError::OutputTooSmall => LZVOLT_E_OUTPUT_TOO_SMALL,
    }
}

/// Borrow a caller's buffer as a slice, or `None` if the pointer lies.
///
/// A zero length is an empty slice whatever the pointer, which is what a C
/// caller means by `(NULL, 0)`. Anything else with a null pointer is refused.
///
/// # Safety
///
/// `ptr` must be readable for `len` bytes when `len` is non-zero.
unsafe fn as_slice<'a>(ptr: *const u8, len: usize) -> Option<&'a [u8]> {
    if len == 0 {
        return Some(&[]);
    }
    if ptr.is_null() {
        return None;
    }
    Some(std::slice::from_raw_parts(ptr, len))
}

/// The mutable form of [`as_slice`].
///
/// # Safety
///
/// `ptr` must be writable for `len` bytes when `len` is non-zero, and must not
/// alias anything else in scope.
unsafe fn as_slice_mut<'a>(ptr: *mut u8, len: usize) -> Option<&'a mut [u8]> {
    if len == 0 {
        return Some(&mut []);
    }
    if ptr.is_null() {
        return None;
    }
    Some(std::slice::from_raw_parts_mut(ptr, len))
}

thread_local! {
    /// Where [`lzvolt_compress`] builds its result before copying it out.
    ///
    /// The compressor writes through a `Vec`, pushing at both ends of the
    /// stream, so unlike the decoder it cannot be pointed at a caller's
    /// buffer without restructuring it. Reusing one buffer per thread keeps
    /// that to a single copy of the *compressed* bytes and no allocation after
    /// the first call.
    static SCRATCH: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

/// The largest stream [`lzvolt_compress`] can produce for this input.
///
/// A buffer this size never runs out. In practice the answer is far smaller —
/// the encoder refuses anything it cannot shrink by an eighth — but the bound
/// has to cover the shapes it does accept.
#[no_mangle]
pub extern "C" fn lzvolt_compress_bound(src_len: usize) -> usize {
    // Four header bytes, the worst-case body, and the hybrid trailer.
    src_len + src_len / 4 * 3 + src_len / 128 + 4 + 8 + 64
}

/// How much room [`lzvolt_decompress`] wants for a value of `declared` bytes.
///
/// A buffer of exactly `declared` decodes the same bytes. This one lets the
/// decoders copy in wide blocks, which on incompressible 4 KiB was worth 34%.
#[no_mangle]
pub extern "C" fn lzvolt_decompress_bound(declared: usize) -> usize {
    format::decompressed_bound(declared)
}

/// Compress `src` into `dst`.
///
/// Returns the number of bytes written, [`LZVOLT_NOT_COMPRESSIBLE`] (zero) if
/// the encoder declined, or a negative error code. `dst_cap` should be at
/// least [`lzvolt_compress_bound`]; a smaller buffer that the result does not
/// fit into is [`LZVOLT_E_OUTPUT_TOO_SMALL`].
///
/// # Safety
///
/// `src` must be readable for `src_len` bytes and `dst` writable for
/// `dst_cap`, and the two must not overlap.
#[no_mangle]
pub unsafe extern "C" fn lzvolt_compress(
    src: *const u8,
    src_len: usize,
    dst: *mut u8,
    dst_cap: usize,
) -> i64 {
    let Some(input) = as_slice(src, src_len) else {
        return LZVOLT_E_NULL;
    };
    let Some(out) = as_slice_mut(dst, dst_cap) else {
        return LZVOLT_E_NULL;
    };
    SCRATCH.with(|cell| {
        let mut buf = cell.borrow_mut();
        if !format::pack(input, &mut buf) {
            return LZVOLT_NOT_COMPRESSIBLE;
        }
        if buf.len() > out.len() {
            return LZVOLT_E_OUTPUT_TOO_SMALL;
        }
        out[..buf.len()].copy_from_slice(&buf);
        buf.len() as i64
    })
}

/// The uncompressed length a stream declares, without decoding it.
///
/// Validates the header on the way, so a stream this rejects is one
/// [`lzvolt_decompress`] would reject too.
///
/// # Safety
///
/// `src` must be readable for `src_len` bytes.
#[no_mangle]
pub unsafe extern "C" fn lzvolt_declared_size(src: *const u8, src_len: usize) -> i64 {
    let Some(input) = as_slice(src, src_len) else {
        return LZVOLT_E_NULL;
    };
    match format::declared_len(input) {
        Ok(n) => n as i64,
        Err(e) => code_for(e),
    }
}

/// Decompress `src` into `dst`, returning the number of bytes written.
///
/// Writes directly into `dst`; nothing is copied afterwards. `dst_cap` must be
/// at least the declared length — ask [`lzvolt_declared_size`] — and should be
/// [`lzvolt_decompress_bound`] of it for the wide copies.
///
/// # Safety
///
/// `src` must be readable for `src_len` bytes and `dst` writable for
/// `dst_cap`, and the two must not overlap.
#[no_mangle]
pub unsafe extern "C" fn lzvolt_decompress(
    src: *const u8,
    src_len: usize,
    dst: *mut u8,
    dst_cap: usize,
) -> i64 {
    let Some(input) = as_slice(src, src_len) else {
        return LZVOLT_E_NULL;
    };
    let Some(out) = as_slice_mut(dst, dst_cap) else {
        return LZVOLT_E_NULL;
    };
    match format::unpack_into_slice(input, out) {
        Ok(n) => n as i64,
        Err(e) => code_for(e),
    }
}

/// Decompress a raw LZ4 block, whose length the caller has to know.
///
/// This is the LZ4 block format with no frame, magic or checksum — what
/// `LZ4_compress_default` writes and `LZ4_decompress_safe` reads. The block
/// does not carry its uncompressed size, which is why `declared` is an
/// argument here and not for [`lzvolt_decompress`].
///
/// # Safety
///
/// `src` must be readable for `src_len` bytes and `dst` writable for
/// `dst_cap`, and the two must not overlap.
#[no_mangle]
pub unsafe extern "C" fn lzvolt_decompress_lz4(
    src: *const u8,
    src_len: usize,
    dst: *mut u8,
    dst_cap: usize,
    declared: usize,
) -> i64 {
    let Some(block) = as_slice(src, src_len) else {
        return LZVOLT_E_NULL;
    };
    let Some(out) = as_slice_mut(dst, dst_cap) else {
        return LZVOLT_E_NULL;
    };
    if out.len() < declared {
        return LZVOLT_E_OUTPUT_TOO_SMALL;
    }
    // The LZ4 path has no header to read, so it goes through the Vec entry
    // point and copies. Unlike our own format there is no slice-shaped
    // decoder behind it yet; see `unpack_into` in src/format.rs.
    SCRATCH.with(|cell| {
        let mut buf = cell.borrow_mut();
        match format::unpack_into(block, &mut buf, declared) {
            Ok(()) => {
                out[..buf.len()].copy_from_slice(&buf);
                buf.len() as i64
            }
            Err(e) => code_for(e),
        }
    })
}

/// The name of the code path this build decodes with, as a C string.
///
/// `"aarch64 assembly"`, `"x86-64 assembly"`, or the portable fallback. A
/// measurement that cannot be traced to the path that produced it is worth
/// nothing, and this is how a C harness records it.
#[no_mangle]
pub extern "C" fn lzvolt_backend() -> *const c_char {
    let s: &'static std::ffi::CStr = match crate::backend() {
        "aarch64 assembly" => c"aarch64 assembly",
        "x86-64 assembly" => c"x86-64 assembly",
        "neon intrinsics" => c"neon intrinsics",
        "sse2 intrinsics" => c"sse2 intrinsics",
        _ => c"scalar",
    };
    s.as_ptr()
}

/// This library's version, as a C string: `"0.1.0"`.
#[no_mangle]
pub extern "C" fn lzvolt_version() -> *const c_char {
    concat!(env!("CARGO_PKG_VERSION"), "\0").as_ptr() as *const c_char
}

/// A short description of an `LZVOLT_E_*` code, as a C string.
///
/// Never null, and never owned by the caller — these are static.
#[no_mangle]
pub extern "C" fn lzvolt_strerror(code: i64) -> *const c_char {
    let s: &'static std::ffi::CStr = match code {
        LZVOLT_NOT_COMPRESSIBLE => c"not compressible; store the bytes as they are",
        LZVOLT_E_NULL => c"null pointer with a non-zero length",
        LZVOLT_E_TRUNCATED => c"packed value ends mid-field",
        LZVOLT_E_BAD_OFFSET => c"backward reference points before the output",
        LZVOLT_E_LENGTH_MISMATCH => c"packed value does not produce its declared length",
        LZVOLT_E_TOO_LARGE => c"packed value declares an implausible length",
        LZVOLT_E_NON_CANONICAL => c"header is longer than its length needs",
        LZVOLT_E_OUTPUT_TOO_SMALL => c"output buffer is smaller than the declared length",
        _ => c"unknown error code",
    };
    s.as_ptr()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The C entry points agree with the Rust ones they wrap.
    ///
    /// Called through the pointers a C caller would pass, because the whole
    /// point of this layer is the pointer handling: a slice built from the
    /// wrong length or a null slipped past a check is the failure mode, and it
    /// is not visible from the Rust side.
    #[test]
    fn the_c_entry_points_round_trip() {
        let data: Vec<u8> = (0..4096u32)
            .map(|i| b"the quick brown fox "[(i % 20) as usize])
            .collect();

        let mut packed = vec![0u8; lzvolt_compress_bound(data.len())];
        let n = unsafe { lzvolt_compress(data.as_ptr(), data.len(), packed.as_mut_ptr(), packed.len()) };
        assert!(n > 0, "compress returned {n}");
        let n = n as usize;

        let declared = unsafe { lzvolt_declared_size(packed.as_ptr(), n) };
        assert_eq!(declared, data.len() as i64);

        let mut out = vec![0u8; lzvolt_decompress_bound(data.len())];
        let got = unsafe { lzvolt_decompress(packed.as_ptr(), n, out.as_mut_ptr(), out.len()) };
        assert_eq!(got, data.len() as i64);
        assert_eq!(&out[..data.len()], &data[..]);
    }

    /// Every way a caller can get the arguments wrong is a code, not a crash.
    #[test]
    fn the_c_entry_points_refuse_rather_than_crash() {
        let mut dst = [0u8; 64];

        // Null with a length that claims bytes.
        assert_eq!(
            unsafe { lzvolt_decompress(std::ptr::null(), 8, dst.as_mut_ptr(), dst.len()) },
            LZVOLT_E_NULL
        );
        assert_eq!(
            unsafe { lzvolt_compress(std::ptr::null(), 8, dst.as_mut_ptr(), dst.len()) },
            LZVOLT_E_NULL
        );
        // Null output with a claimed capacity.
        assert_eq!(
            unsafe { lzvolt_decompress(dst.as_ptr(), 4, std::ptr::null_mut(), 8) },
            LZVOLT_E_NULL
        );

        // Null with zero length is an empty slice, which is not an error --
        // it is a stream too short to be one.
        let r = unsafe { lzvolt_declared_size(std::ptr::null(), 0) };
        assert_eq!(r, LZVOLT_E_TRUNCATED);

        // A stream that decodes, into a buffer one byte too small.
        let data = vec![b'z'; 512];
        let mut packed = vec![0u8; lzvolt_compress_bound(data.len())];
        let n = unsafe {
            lzvolt_compress(data.as_ptr(), data.len(), packed.as_mut_ptr(), packed.len())
        } as usize;
        let mut small = vec![0u8; data.len() - 1];
        assert_eq!(
            unsafe { lzvolt_decompress(packed.as_ptr(), n, small.as_mut_ptr(), small.len()) },
            LZVOLT_E_OUTPUT_TOO_SMALL
        );

        // And compressing into a buffer the result does not fit in.
        let mut cramped = [0u8; 4];
        assert_eq!(
            unsafe {
                lzvolt_compress(data.as_ptr(), data.len(), cramped.as_mut_ptr(), cramped.len())
            },
            LZVOLT_E_OUTPUT_TOO_SMALL
        );
    }

    /// Incompressible input is a zero, not an error, and not a stream.
    #[test]
    fn the_c_compressor_says_no_the_way_c_expects() {
        let mut state = 0x9E37_79B9_7F4A_7C15u64;
        let noise: Vec<u8> = (0..4096)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                state as u8
            })
            .collect();
        let mut out = vec![0u8; lzvolt_compress_bound(noise.len())];
        let n =
            unsafe { lzvolt_compress(noise.as_ptr(), noise.len(), out.as_mut_ptr(), out.len()) };
        assert_eq!(n, LZVOLT_NOT_COMPRESSIBLE);
    }

    /// The strings are real C strings and the version is the crate's.
    #[test]
    fn the_c_strings_are_nul_terminated() {
        let v = unsafe { std::ffi::CStr::from_ptr(lzvolt_version()) };
        assert_eq!(v.to_str().unwrap(), env!("CARGO_PKG_VERSION"));

        let b = unsafe { std::ffi::CStr::from_ptr(lzvolt_backend()) };
        assert_eq!(b.to_str().unwrap(), crate::backend());

        for code in [
            LZVOLT_NOT_COMPRESSIBLE,
            LZVOLT_E_NULL,
            LZVOLT_E_TRUNCATED,
            LZVOLT_E_BAD_OFFSET,
            LZVOLT_E_LENGTH_MISMATCH,
            LZVOLT_E_TOO_LARGE,
            LZVOLT_E_NON_CANONICAL,
            LZVOLT_E_OUTPUT_TOO_SMALL,
            -999,
        ] {
            let s = unsafe { std::ffi::CStr::from_ptr(lzvolt_strerror(code)) };
            assert!(!s.to_bytes().is_empty(), "no text for {code}");
        }
    }
}
