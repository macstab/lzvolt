//! The decoder, in assembly where one exists.
//!
//! This is the read path. A value is packed once when it is written and
//! unpacked on every read after that, so a cycle here is worth more than a
//! cycle in the packer by whatever the read/write ratio of the workload is —
//! which for a cache is not close to one.
//!
//! # What it leaves to Rust
//!
//! The length header and every error message. The kernel is handed a body and a
//! length and answers one question: did it decode. On anything else — a
//! malformed stream, a buffer without the slack the checks assume — it returns
//! zero and [`unpack_asm`] reports that the caller should run the portable
//! decoder, which produces the precise error. A stream only fails when it is
//! corrupt, so decoding it twice costs nothing that matters, and there is one
//! definition of every error rather than two that can drift.
//!
//! Leaving the varint outside is the same trade. It runs once per value, so
//! moving it into assembly would buy nothing measurable and would add a second
//! implementation of an encoding that has to agree exactly.

/// Bytes of headroom the kernel needs past the declared length.
///
/// The Rust decoder re-asks `produced + 64 <= capacity` at every block. Giving
/// the kernel this much slack up front turns that into a question it can answer
/// once: `produced < declared` holds at every block, so `produced + 64 <
/// declared + 64 <= capacity` needs no test inside the loop. The block copies
/// then overrun freely into the slack, and the length is set to the declared
/// size at the end, so nothing ever reads what they wrote past it.
pub const UNPACK_SLACK: usize = 64;

#[cfg(all(keva_asm, any(target_arch = "aarch64", target_arch = "x86_64")))]
extern "C" {
    fn keva_unpack(
        src: *const u8,
        src_len: usize,
        dst: *mut u8,
        dst_cap: usize,
        declared: usize,
        start: usize,
    ) -> u32;

    /// The same decoder assembled for the wide split -- two bits of literal
    /// length, six of match. See `asm/aarch64/unpack.S`.
    fn keva_unpack_wide(
        src: *const u8,
        src_len: usize,
        dst: *mut u8,
        dst_cap: usize,
        declared: usize,
        start: usize,
    ) -> u32;
}

/// The same two kernels with 256-bit moves.
///
/// Same format, same bounds, same branches -- only the fixed match move and the
/// block copies are wider, which on x86 is two micro-ops where SSE2 needs four,
/// and six where it needs twelve under the wide split. AVX2 is not part of the
/// x86-64 baseline, so which pair runs is decided once, on the first call.
#[cfg(all(keva_asm, target_arch = "x86_64"))]
extern "C" {
    fn keva_unpack_avx2(
        src: *const u8,
        src_len: usize,
        dst: *mut u8,
        dst_cap: usize,
        declared: usize,
        start: usize,
    ) -> u32;

    fn keva_unpack_wide_avx2(
        src: *const u8,
        src_len: usize,
        dst: *mut u8,
        dst_cap: usize,
        declared: usize,
        start: usize,
    ) -> u32;
}

/// Run the kernel for `split`, widest first.
///
/// A branch rather than a function pointer: at 512 bytes the fixed cost of a
/// call is a large share of the work, and an indirect one through a table would
/// add to exactly the case that can least afford it. The condition is a cached
/// bool and predicts perfectly after the first call.
///
/// # Safety
///
/// The caller owes what the kernels' contract asks: `dst_cap` bytes writable at
/// `dst`, `src_len` readable at `src`.
#[cfg(all(keva_asm, any(target_arch = "aarch64", target_arch = "x86_64")))]
#[inline]
unsafe fn run(
    split: Split,
    src: *const u8,
    src_len: usize,
    dst: *mut u8,
    dst_cap: usize,
    declared: usize,
    start: usize,
) -> u32 {
    #[cfg(target_arch = "x86_64")]
    if crate::cpu::features().avx2 {
        return match split {
            Split::Even => keva_unpack_avx2(src, src_len, dst, dst_cap, declared, start),
            Split::WideMatch => keva_unpack_wide_avx2(src, src_len, dst, dst_cap, declared, start),
        };
    }
    match split {
        Split::Even => keva_unpack(src, src_len, dst, dst_cap, declared, start),
        Split::WideMatch => keva_unpack_wide(src, src_len, dst, dst_cap, declared, start),
    }
}

/// Which way a value's token divides its eight bits.
///
/// Assembly is specialised per split rather than parameterised, so this selects
/// between two kernels rather than setting a register in one.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Split {
    /// Four bits each, which is also the LZ4 block format.
    Even,
    /// Two for the literal run, six for the match.
    WideMatch,
}

/// Decode into a caller-provided buffer, the way `LZ4_decompress_safe` is used.
///
/// The exact counterpart of the C entry point: a source, a destination, its
/// capacity, and the uncompressed length. No allocation, no `Vec` bookkeeping,
/// no `Result` — one call and a boolean. It exists so a decoder-against-decoder
/// measurement has nothing but the two decoders in it.
///
/// `dst` must be at least `declared + UNPACK_SLACK` long; the kernel refuses
/// otherwise. Returns whether it decoded; a `false` means the caller should run
/// the portable decoder, exactly as with [`unpack_asm`].
pub fn unpack_into_slice(src: &[u8], dst: &mut [u8], declared: usize, split: Split) -> bool {
    unpack_section(src, dst, declared, 0, split)
}

/// Decode one section of a value, producing `dst[start..declared]`.
///
/// A value whose token split changes partway is two sections, decoded by two
/// calls into the two kernels. The second is handed the same `dst` and the
/// output position the first stopped at, so its backward references reach into
/// what the first wrote -- the window does not restart at the boundary.
///
/// `start == 0` is the whole-value case and is what [`unpack_into_slice`] and
/// [`unpack_asm`] use.
pub fn unpack_section(
    src: &[u8],
    dst: &mut [u8],
    declared: usize,
    start: usize,
    split: Split,
) -> bool {
    #[cfg(not(all(keva_asm, any(target_arch = "aarch64", target_arch = "x86_64"))))]
    {
        let _ = (src, dst, declared, start, split);
        false
    }

    #[cfg(all(keva_asm, any(target_arch = "aarch64", target_arch = "x86_64")))]
    {
        if declared == 0 || start >= declared {
            return false;
        }
        // SAFETY: `dst.len()` is the true length of the buffer, and the kernel
        // writes below it or returns zero.
        let produced = unsafe {
            run(
                split,
                src.as_ptr(),
                src.len(),
                dst.as_mut_ptr(),
                dst.len(),
                declared,
                start,
            )
        } as usize;
        produced == declared
    }
}

/// Decode a value whose token split changes partway, as two kernel calls.
///
/// `switch` is `(offset into body, output position)` where the second section
/// begins. The two calls share one destination, so the second section's
/// backward references reach into what the first wrote -- the match window does
/// not restart at the boundary, which is what makes switching cheap enough to
/// be worth doing at all.
///
/// Both offsets come off the wire and are treated as hostile: they are checked
/// against the body and the declared length here, and everything past that is
/// the kernel's own bounds. A `false` means "run the portable decoder", exactly
/// as with [`unpack_asm`].
pub fn unpack_asm_hybrid(
    body: &[u8],
    out: &mut Vec<u8>,
    declared: usize,
    switch: (usize, usize),
    first: Split,
    second: Split,
) -> bool {
    #[cfg(not(all(keva_asm, any(target_arch = "aarch64", target_arch = "x86_64"))))]
    {
        let _ = (body, out, declared, switch, first, second);
        false
    }

    #[cfg(all(keva_asm, any(target_arch = "aarch64", target_arch = "x86_64")))]
    {
        let (in_at, out_at) = switch;
        if declared == 0 || out_at == 0 || out_at >= declared || in_at > body.len() {
            return false;
        }

        out.clear();
        // `Vec::reserve` is a call whose body handles growth, and the caller
        // reuses its buffer, so growth is the case that never happens. Asking
        // about the capacity first keeps the common path to a compare.
        let want = declared + UNPACK_SLACK;
        if out.capacity() < want {
            out.reserve(want);
        }
        let cap = out.capacity();

        // SAFETY: `cap` is the real capacity and is at least `declared + 64`,
        // which the kernel re-checks. The first call is given `out_at` as its
        // declared length, so it writes below `out_at + 64 <= declared + 64`;
        // the second starts at `out_at` and stops at `declared`. Nothing reads
        // the slack: the length is only ever set to `declared`.
        let ok = unsafe {
            let dst = out.as_mut_ptr();
            let first_len = run(first, body.as_ptr(), in_at, dst, cap, out_at, 0) as usize;
            if first_len != out_at {
                false
            } else {
                let tail = body.as_ptr().add(in_at);
                let tail_len = body.len() - in_at;
                let total = run(second, tail, tail_len, dst, cap, declared, out_at) as usize;
                total == declared
            }
        };

        if !ok {
            out.clear();
            return false;
        }
        // SAFETY: both calls reported reaching their targets, so every byte
        // below `declared` was written by one of them.
        unsafe { out.set_len(declared) };
        true
    }
}

/// Whether this build has an assembly decoder.
pub const fn asm_available() -> bool {
    cfg!(all(
        keva_asm,
        any(target_arch = "aarch64", target_arch = "x86_64")
    ))
}

/// Decode `body` — the packed stream with its length header already removed —
/// into `out`, which is replaced.
///
/// Returns `false` to mean "use the portable decoder": either there is no
/// kernel on this target, or the kernel declined. It never means the value is
/// definitely corrupt, so the caller must fall back rather than report an
/// error.
pub fn unpack_asm(body: &[u8], out: &mut Vec<u8>, declared: usize, split: Split) -> bool {
    #[cfg(not(all(keva_asm, any(target_arch = "aarch64", target_arch = "x86_64"))))]
    {
        let _ = (body, out, declared, split);
        false
    }

    #[cfg(all(keva_asm, any(target_arch = "aarch64", target_arch = "x86_64")))]
    {
        // Zero is how the kernel says it declined, so it cannot also be how it
        // reports success. An empty value is the portable decoder's business.
        if declared == 0 {
            return false;
        }

        out.clear();
        // `Vec::reserve` is a call whose body handles growth, and the caller
        // reuses its buffer, so growth is the case that never happens. Asking
        // about the capacity first keeps the common path to a compare.
        let want = declared + UNPACK_SLACK;
        if out.capacity() < want {
            out.reserve(want);
        }
        let cap = out.capacity();

        // SAFETY: `cap` is the real capacity, so the whole range handed to the
        // kernel is allocated. The kernel writes below `declared + 64` or
        // returns zero, and it is handed `cap` so it can verify that itself.
        // Nothing reads the slack: the length only moves out to `declared`, and
        // every byte below it was written by this call.
        let produced = unsafe {
            run(
                split,
                body.as_ptr(),
                body.len(),
                out.as_mut_ptr(),
                cap,
                declared,
                0,
            )
        } as usize;

        if produced != declared {
            out.clear();
            return false;
        }

        // SAFETY: the kernel reported writing exactly `declared` bytes from the
        // pointer, and `declared + 64 <= cap` was checked before it started.
        unsafe { out.set_len(declared) };
        true
    }
}

#[cfg(all(test, keva_asm, target_arch = "x86_64"))]
mod avx2 {
    use super::*;

    /// The 256-bit kernels must agree with the 128-bit ones, byte for byte.
    ///
    /// They are the same algorithm with wider moves, so anything else is a bug
    /// in the widening. This is the only test that reaches them: which pair
    /// runs is a run-time choice, and a machine without AVX2 -- a translator,
    /// for one -- takes the narrow pair for everything and would report green
    /// having never executed a line of the wide one.
    ///
    /// So it skips loudly rather than quietly, and calls both pairs directly
    /// rather than going through the dispatch that would hide the difference.
    #[test]
    fn the_wide_kernels_agree_with_the_narrow_ones() {
        if !crate::cpu::features().avx2 {
            eprintln!("  no AVX2 here: the 256-bit kernels were NOT exercised");
            return;
        }

        // Streams the packer would produce, built here so this crate needs no
        // dependency on the one that writes them: a token, a literal run, an
        // offset, and lengths that reach past a nibble in both fields.
        let mut cases: Vec<(Vec<u8>, usize, Split)> = Vec::new();
        for &(lit, mat, off) in &[
            (0usize, 4usize, 1usize),
            (1, 4, 1),
            (3, 8, 3),
            (14, 18, 16),
            (14, 18, 40),
            (5, 60, 7),
            (5, 200, 100),
            (300, 4, 250),
        ] {
            for split in [Split::Even, Split::WideMatch] {
                let (lit_bits, lit_max, mat_max) = match split {
                    Split::Even => (4u32, 15usize, 15usize),
                    Split::WideMatch => (6, 3, 63),
                };
                // The match must reach no further back than what precedes it.
                if off > lit || off == 0 {
                    continue;
                }
                let mut body = Vec::new();
                let short_lit = lit.min(lit_max);
                let short_mat = (mat - 4).min(mat_max);
                body.push(((short_lit << (8 - lit_bits)) | short_mat) as u8);
                if short_lit == lit_max {
                    let mut rest = lit - lit_max;
                    while rest >= 255 {
                        body.push(255);
                        rest -= 255;
                    }
                    body.push(rest as u8);
                }
                body.extend((0..lit).map(|i| (i % 251) as u8 + 1));
                body.extend_from_slice(&(off as u16).to_le_bytes());
                if short_mat == mat_max {
                    let mut rest = (mat - 4) - mat_max;
                    while rest >= 255 {
                        body.push(255);
                        rest -= 255;
                    }
                    body.push(rest as u8);
                }
                // A closing literal run, which is what ends a stream.
                let tail = 20usize;
                body.push((tail.min(lit_max) << (8 - lit_bits)) as u8);
                if tail >= lit_max {
                    let mut rest = tail - lit_max;
                    while rest >= 255 {
                        body.push(255);
                        rest -= 255;
                    }
                    body.push(rest as u8);
                }
                body.extend((0..tail).map(|i| (i % 253) as u8 + 1));
                cases.push((body, lit + mat + tail, split));
            }
        }

        let mut checked = 0usize;
        for (body, declared, split) in &cases {
            let cap = declared + UNPACK_SLACK;
            let mut narrow = vec![0u8; cap];
            let mut wide = vec![0u8; cap];

            // SAFETY: both buffers are `cap` long and the kernels are told so.
            let (a, b) = unsafe {
                let n = match split {
                    Split::Even => keva_unpack(
                        body.as_ptr(),
                        body.len(),
                        narrow.as_mut_ptr(),
                        cap,
                        *declared,
                        0,
                    ),
                    Split::WideMatch => keva_unpack_wide(
                        body.as_ptr(),
                        body.len(),
                        narrow.as_mut_ptr(),
                        cap,
                        *declared,
                        0,
                    ),
                };
                let w = match split {
                    Split::Even => keva_unpack_avx2(
                        body.as_ptr(),
                        body.len(),
                        wide.as_mut_ptr(),
                        cap,
                        *declared,
                        0,
                    ),
                    Split::WideMatch => keva_unpack_wide_avx2(
                        body.as_ptr(),
                        body.len(),
                        wide.as_mut_ptr(),
                        cap,
                        *declared,
                        0,
                    ),
                };
                (n, w)
            };

            assert_eq!(a, b, "the two widths disagree on whether a stream decodes");
            if a as usize == *declared {
                assert_eq!(
                    narrow[..*declared],
                    wide[..*declared],
                    "{declared} bytes, {split:?}: the 256-bit kernel produced different bytes"
                );
                checked += 1;
            }
        }
        assert!(checked > 0, "no case decoded, so nothing was compared");
        eprintln!("  {checked} streams agreed between the 128- and 256-bit kernels");
    }
}
