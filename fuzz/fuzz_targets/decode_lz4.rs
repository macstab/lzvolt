//! Arbitrary bytes as a raw LZ4 block.
//!
//! An LZ4 block carries no length of its own, so the caller supplies one — and
//! that makes this the more dangerous of the two entry points. The decoder is
//! being told how much to write by one input and what to write by another, and
//! nothing forces the two to agree. The native path at least reads its length
//! from the same bytes it then validates.
//!
//! This is also the entry point a migrating caller reaches first, pointing it
//! at blocks some other library wrote.

#![no_main]

use libfuzzer_sys::fuzz_target;

const DECODE_CAP: usize = 8 * 1024 * 1024;

fuzz_target!(|data: &[u8]| {
    // First two bytes choose the declared length, the rest is the block. A
    // length drawn from the same bytes as the body lets the search correlate
    // the two, which is how it finds the case where they nearly agree.
    if data.len() < 2 {
        return;
    }
    let (head, block) = data.split_at(2);
    let declared = u16::from_le_bytes([head[0], head[1]]) as usize;

    // Scale a 16-bit draw up past the window so matches that reach back the
    // full 64 KiB are reachable, without letting the allocation run away.
    let declared = (declared * 97).min(DECODE_CAP);

    let mut out = Vec::new();
    if lzvolt::decompress_lz4(block, &mut out, declared).is_ok() {
        assert_eq!(
            out.len(),
            declared,
            "block was told {declared} bytes and produced {}",
            out.len()
        );
    }
});
