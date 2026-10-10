//! The caller-owned-buffer path, including buffers that are too small.
//!
//! `decompress_into` is the zero-copy entry point, and the only one that can
//! return OutputTooSmall. It is also the one where a caller gets the size
//! wrong: the decoders copy in wide chunks and want `decompressed_bound` of
//! room, not `declared`. Handing it exactly `declared` is the mistake a reader
//! of the signature alone would make, so it is worth fuzzing on purpose rather
//! than assuming nobody will.

#![no_main]

use libfuzzer_sys::fuzz_target;

const DECODE_CAP: usize = 8 * 1024 * 1024;

fuzz_target!(|data: &[u8]| {
    if data.is_empty() {
        return;
    }
    // One byte decides how wrong the buffer is; the rest is the stream.
    let (choice, stream) = data.split_at(1);
    let declared = match lzvolt::decompressed_size(stream) {
        Ok(n) => n,
        Err(_) => return,
    };
    if declared > DECODE_CAP {
        return;
    }

    let bound = lzvolt::decompressed_bound(declared);
    let cap = match choice[0] % 4 {
        0 => bound,                      // what the API asks for
        1 => declared,                   // what a careless caller passes
        2 => declared.saturating_sub(1), // one short
        _ => bound / 2,                  // comfortably short
    };

    let mut out = vec![0u8; cap];
    match lzvolt::decompress_into(stream, &mut out) {
        Ok(n) => {
            assert_eq!(n, declared, "wrote {n} bytes for a declared {declared}");
            assert!(n <= cap, "wrote {n} bytes into a buffer of {cap}");
        }
        Err(_) => {
            // A refusal is fine whatever the reason. What is not fine is
            // refusing after having already written past the end, and that is
            // what the allocator's own redzones catch under ASan.
        }
    }
});
