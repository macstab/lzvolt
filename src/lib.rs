//! LZ, variable: a byte-oriented LZ77 codec that picks its token layout per
//! value instead of fixing one for all data.
//!
//! The `v` is the whole idea. LZ4 spends four bits of every token on the
//! literal length and four on the match length, once, for everything it will
//! ever compress. `lzvolt` carries two layouts and writes in the header which
//! one a value used -- and a value may switch partway.
//!
//! `lzvolt` compresses and decompresses small-to-medium values — the sizes a cache
//! or a key-value store actually holds — and it reads and writes LZ4 blocks as
//! well, so it can be dropped in front of data somebody else compressed.
//!
//! ```
//! let data = br#"{"id":1,"tenant":"tenant42","role":"member"}"#;
//!
//! let mut packed = Vec::new();
//! if lzvolt::compress(data, &mut packed) {
//!     let mut out = Vec::new();
//!     lzvolt::decompress(&packed, &mut out).unwrap();
//!     assert_eq!(out, data);
//! }
//! ```
//!
//! # What is different about it
//!
//! **The token layout is chosen per value.** LZ4 spends four bits of every
//! token on the literal length and four on the match length. That split is a
//! compromise across all data, and for data that compresses well it is the
//! wrong one: counting real blocks, the match length overflows four bits in 99%
//! of them at a six-fold ratio. `lzvolt` carries two layouts — four/four, and two
//! literal bits with five match bits and one bit meaning "the offset the last
//! block used" — and writes in the header which one a value used. A value may
//! even switch partway, which is one call per section to the decoder.
//!
//! **The decoder is hand-written per part line, not per architecture.** There
//! are separate assembled bodies for Intel's server line, AMD's, an
//! unrecognised SSSE3 part, Arm's Neoverse V2 and everything else AArch64,
//! selected at run time from CPUID and MIDR. They exist because the same change
//! has measured opposite signs on two parts often enough to stop treating "x86"
//! as one target: the last literal run wants `rep movsb` on a Xeon and a copy
//! ladder on an EPYC, and Neoverse V2 wants its source aligned where an Apple
//! part does not care.
//!
//! Every number this crate claims was measured on rented hardware and is
//! recorded, with the refutations, in `docs/MEASUREMENTS.md`. The format itself
//! is specified in `docs/FORMAT.md`, which is normative where this
//! implementation is not.
//!
//! # The four entry points
//!
//! [`compress`] and [`decompress`] are the codec. [`decompress_lz4`] reads a
//! block somebody else's LZ4 wrote. [`backend`] says which body this machine
//! selected, which is what a benchmark result should be labelled with.
//!
//! Everything else is in [`raw`], and none of it is stable.

pub mod cpu;
pub mod format;
pub mod kernel;

/// The C ABI, for callers that are not Rust. See `include/lzvolt.h`.
///
/// Always present: the symbols cost a Rust caller nothing in an `rlib`, and
/// putting them behind a feature would let the published shared library and
/// the published crate disagree about what exists.
pub mod ffi;

pub use format::PackError as Error;

/// Compress `input` into `out`, appending nothing if it is not worth it.
///
/// Returns `false` when the value does not compress — incompressible data, or
/// data too short for a backward reference to pay for itself. The caller is
/// expected to store the original bytes in that case, which is strictly cheaper
/// than a format that must wrap them: there is no header to write and no copy to
/// make on the way back out.
///
/// `out` is appended to, not cleared.
#[inline]
pub fn compress(input: &[u8], out: &mut Vec<u8>) -> bool {
    format::pack(input, out)
}

/// Decompress a stream [`compress`] produced.
///
/// `out` is appended to. The declared length is read from the stream's header,
/// so no capacity hint is needed; a stream that declares a length it does not
/// produce is [`Error::LengthMismatch`] rather than a truncated answer.
#[inline]
pub fn decompress(input: &[u8], out: &mut Vec<u8>) -> Result<(), Error> {
    format::unpack(input, out)
}

/// Decompress into a buffer you already have, returning the bytes written.
///
/// The same decoder [`decompress`] runs, writing straight into `out` with no
/// copy afterwards. Use it when the destination is already allocated — an
/// arena, a page, a slot in a cache — which is the case this codec was built
/// for.
///
/// `out` must be at least [`decompressed_size`] of the stream, and should be
/// [`decompressed_bound`] of that for the wide copies.
#[inline]
pub fn decompress_into(input: &[u8], out: &mut [u8]) -> Result<usize, Error> {
    format::unpack_into_slice(input, out)
}

/// The uncompressed length a stream declares, without decoding it.
///
/// Validates the header, so what this rejects [`decompress`] would reject too.
#[inline]
pub fn decompressed_size(input: &[u8]) -> Result<usize, Error> {
    format::declared_len(input)
}

/// How much room [`decompress_into`] wants for a value of `declared` bytes.
///
/// Exactly `declared` decodes the same bytes; this much lets the decoders copy
/// in wide blocks, which on incompressible 4 KiB was worth 34%.
#[inline]
pub const fn decompressed_bound(declared: usize) -> usize {
    format::decompressed_bound(declared)
}

/// Decompress an LZ4 block, with the length known from somewhere else.
///
/// This is the raw LZ4 block format — no frame, no magic, no checksum — which
/// is what `LZ4_compress_default` writes and what `LZ4_decompress_safe` reads.
/// `declared` is the uncompressed size, which the LZ4 block format does not
/// carry and the caller therefore has to know.
///
/// `out` is appended to.
#[inline]
pub fn decompress_lz4(block: &[u8], out: &mut Vec<u8>, declared: usize) -> Result<(), Error> {
    format::unpack_into(block, out, declared)
}

/// Which assembled body this machine selected, for labelling a measurement.
///
/// A number from this crate means little without it: the bodies differ in their
/// copy widths and their thresholds, and two of them exist precisely because the
/// same source measured opposite signs on two parts.
#[inline]
pub fn backend() -> &'static str {
    kernel::backend_name()
}

/// The lower level, and not stable.
///
/// Section decoding, the per-body entry points, the split selector and the
/// kernel dispatch. These are what the tests and the benchmarks reach for, and
/// what a caller with its own buffer management may want. They change without a
/// major version; [`compress`], [`decompress`], [`decompress_lz4`] and
/// [`backend`] do not.
pub mod raw {
    pub use crate::format::{unpack_into, NarrowPacker, Packer, PortablePacker};
    pub use crate::kernel::decode::*;
    pub use crate::kernel::{asm_enabled, backend_name};
}
