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

// One pair per vendor, picked at run time. Declared only where they exist:
// nothing outside x86-64 links them.
#[cfg(all(keva_asm, target_arch = "x86_64"))]
extern "C" {
    fn keva_unpack_intel(
        src: *const u8,
        src_len: usize,
        dst: *mut u8,
        dst_cap: usize,
        declared: usize,
        start: usize,
    ) -> u32;

    fn keva_unpack_wide_intel(
        src: *const u8,
        src_len: usize,
        dst: *mut u8,
        dst_cap: usize,
        declared: usize,
        start: usize,
    ) -> u32;

    fn keva_unpack_amd(
        src: *const u8,
        src_len: usize,
        dst: *mut u8,
        dst_cap: usize,
        declared: usize,
        start: usize,
    ) -> u32;

    fn keva_unpack_wide_amd(
        src: *const u8,
        src_len: usize,
        dst: *mut u8,
        dst_cap: usize,
        declared: usize,
        start: usize,
    ) -> u32;
}

/// Run the kernel for `split`.
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
    // A near match -- offset below sixteen -- is grown a byte at a time without
    // this, and short offsets are what data that only partly repeats is made
    // of. SSSE3 is universal on anything a server has shipped with since 2007,
    // but it is not the x86-64 baseline, so it is asked for rather than assumed.
    // One kernel per house. They diverge where the same change has measured
    // opposite signs on the two -- so far the last literal run, and there is no
    // reason to expect it to be the last such place. Anything that is neither
    // Intel nor AMD, or has no SSSE3, falls through to the baseline pair.
    #[cfg(target_arch = "x86_64")]
    {
        let f = crate::cpu::features();
        if f.ssse3 && f.intel {
            return match split {
                Split::Even => keva_unpack_intel(src, src_len, dst, dst_cap, declared, start),
                Split::WideMatch => {
                    keva_unpack_wide_intel(src, src_len, dst, dst_cap, declared, start)
                }
            };
        }
        if f.ssse3 && f.amd {
            return match split {
                Split::Even => keva_unpack_amd(src, src_len, dst, dst_cap, declared, start),
                Split::WideMatch => {
                    keva_unpack_wide_amd(src, src_len, dst, dst_cap, declared, start)
                }
            };
        }
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
