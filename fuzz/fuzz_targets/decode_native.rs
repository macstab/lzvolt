//! Arbitrary bytes into the native decoder.
//!
//! This is the target that matters. The decoders' copies are unchecked, so the
//! validation in front of them is the only thing between a malformed stream and
//! memory unsafety — and until now that validation had only ever seen
//! truncations and single-bit flips of streams the packer wrote. A
//! coverage-guided search reaches shapes the packer cannot produce at all,
//! which is exactly the input an attacker supplies.
//!
//! Whatever comes back, the one thing that may not happen is a crash.

#![no_main]

use libfuzzer_sys::fuzz_target;

/// Above this, a valid header makes the allocator do the work rather than the
/// decoder, and libFuzzer reports an out-of-memory that says nothing about the
/// code. The decode loop does not care how large the value is; its branches are
/// the same at 64 KiB as at 64 MiB.
const DECODE_CAP: usize = 8 * 1024 * 1024;

fuzz_target!(|data: &[u8]| {
    // declared_len validates the header on its own. Anything it rejects,
    // decompress would reject too, so skipping here loses no coverage.
    let declared = match lzvolt::decompressed_size(data) {
        Ok(n) => n,
        Err(_) => return,
    };
    if declared > DECODE_CAP {
        return;
    }

    let mut out = Vec::new();
    if lzvolt::decompress(data, &mut out).is_ok() {
        // A stream carries its own length. Producing a different number of
        // bytes than it declared is a correctness bug even without a crash,
        // and it is the shape a truncated-output attack would take.
        assert_eq!(
            out.len(),
            declared,
            "stream declared {declared} bytes and decoded {}",
            out.len()
        );
    }
});
