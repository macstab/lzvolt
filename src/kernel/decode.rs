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
//! zero and `unpack_asm` reports that the caller should run the portable
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

#[cfg(all(lzvolt_asm, any(target_arch = "aarch64", target_arch = "x86_64")))]
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

// One pair per part line, picked at run time. Declared only where they exist:
// nothing outside x86-64 links them.
#[cfg(all(lzvolt_asm, target_arch = "x86_64"))]
extern "C" {
    /// Even split, any SSSE3 part whose brand names no line.
    fn keva_unpack_ssse3(
        src: *const u8,
        src_len: usize,
        dst: *mut u8,
        dst_cap: usize,
        declared: usize,
        start: usize,
    ) -> u32;

    /// Wide split, the same unrecognised-part body.
    fn keva_unpack_wide_ssse3(
        src: *const u8,
        src_len: usize,
        dst: *mut u8,
        dst_cap: usize,
        declared: usize,
        start: usize,
    ) -> u32;

    /// Even split, Intel server line. Carries `rep movsb` above REP_MIN,
    /// which Zen would pay tens of cycles of start-up for.
    fn keva_unpack_xeon(
        src: *const u8,
        src_len: usize,
        dst: *mut u8,
        dst_cap: usize,
        declared: usize,
        start: usize,
    ) -> u32;

    /// Wide split, Intel server line.
    fn keva_unpack_wide_xeon(
        src: *const u8,
        src_len: usize,
        dst: *mut u8,
        dst_cap: usize,
        declared: usize,
        start: usize,
    ) -> u32;

    /// Even split, AMD server line. No string instruction: Zen has ERMSB
    /// without Fast Short REP MOV.
    fn keva_unpack_epyc(
        src: *const u8,
        src_len: usize,
        dst: *mut u8,
        dst_cap: usize,
        declared: usize,
        start: usize,
    ) -> u32;

    /// Wide split, AMD server line.
    fn keva_unpack_wide_epyc(
        src: *const u8,
        src_len: usize,
        dst: *mut u8,
        dst_cap: usize,
        declared: usize,
        start: usize,
    ) -> u32;

    /// Foreign LZ4 blocks on an unrecognised SSSE3 part.
    fn keva_unpack_lz4_ssse3(
        src: *const u8,
        src_len: usize,
        dst: *mut u8,
        dst_cap: usize,
        declared: usize,
        start: usize,
    ) -> u32;

    /// Foreign LZ4 blocks, Intel server line. 256-bit literal copy, so AVX2
    /// is asked for before dispatch picks it -- a brand string is not a
    /// capability.
    fn keva_unpack_lz4_xeon(
        src: *const u8,
        src_len: usize,
        dst: *mut u8,
        dst_cap: usize,
        declared: usize,
        start: usize,
    ) -> u32;

    /// Foreign LZ4 blocks, AMD server line. Its memcpy threshold is 8192
    /// where the Xeon body uses 128: Zen 4 reaches its store limit with two
    /// 32-byte moves, so glibc can only arrive later. See
    /// docs/MEASUREMENTS.md.
    fn keva_unpack_lz4_epyc(
        src: *const u8,
        src_len: usize,
        dst: *mut u8,
        dst_cap: usize,
        declared: usize,
        start: usize,
    ) -> u32;
}

// The body for blocks somebody else wrote.
//
// One per part line, no wide variant: LZ4's token is four bits each way, which
// is the even split. Assembled from the same source as the even body and
// currently identical to it -- it exists so the two can diverge where their
// formats' guarantees do. LZ4 promises a block ends in a literal run of at
// least five bytes with no match inside the last twelve; our packer emits
// matches up to the final byte, so nothing derived from that promise may live
// in the body our own values run.
#[cfg(all(lzvolt_asm, any(target_arch = "aarch64", target_arch = "x86_64")))]
extern "C" {
    fn keva_unpack_lz4(
        src: *const u8,
        src_len: usize,
        dst: *mut u8,
        dst_cap: usize,
        declared: usize,
        start: usize,
    ) -> u32;
}

// The AArch64 part line, declared only where it exists.
//
// Neoverse V2's copy loop aligns its destination before it streams and the
// generic one does not, because the same change measures opposite signs on this
// architecture's two parts. See `asm/aarch64/unpack_lz4_neoverse.S`.
#[cfg(all(lzvolt_asm, target_arch = "aarch64"))]
extern "C" {
    fn keva_unpack_lz4_neoverse(
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
#[cfg(all(lzvolt_asm, any(target_arch = "aarch64", target_arch = "x86_64")))]
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
    // One kernel per part line, and the line is not the vendor: a Core laptop
    // and a Sapphire Rapids Xeon share a house and little else. A part that
    // names neither line gets the plain SSSE3 kernel, which assumes nothing
    // about either -- that is a client Intel part, a hypervisor that rewrites
    // the brand string, or a vendor nobody has measured.
    #[cfg(target_arch = "x86_64")]
    {
        let f = crate::cpu::features();
        if f.ssse3 {
            return match (f.xeon, f.epyc, split) {
                (true, _, Split::Even) => {
                    keva_unpack_xeon(src, src_len, dst, dst_cap, declared, start)
                }
                (true, _, Split::WideMatch) => {
                    keva_unpack_wide_xeon(src, src_len, dst, dst_cap, declared, start)
                }
                (_, true, Split::Even) => {
                    keva_unpack_epyc(src, src_len, dst, dst_cap, declared, start)
                }
                (_, true, Split::WideMatch) => {
                    keva_unpack_wide_epyc(src, src_len, dst, dst_cap, declared, start)
                }
                (_, _, Split::Even) => {
                    keva_unpack_ssse3(src, src_len, dst, dst_cap, declared, start)
                }
                (_, _, Split::WideMatch) => {
                    keva_unpack_wide_ssse3(src, src_len, dst, dst_cap, declared, start)
                }
            };
        }
    }

    match split {
        Split::Even => keva_unpack(src, src_len, dst, dst_cap, declared, start),
        Split::WideMatch => keva_unpack_wide(src, src_len, dst, dst_cap, declared, start),
    }
}

/// Which LZ4 body a machine runs.
///
/// The same four the even body is built for, minus the pairing with a split:
/// LZ4 has one token layout. Exposed because dispatch means only one of them
/// executes on a given machine, and a body no test reaches is a body that can
/// diverge unnoticed -- that is exactly how a case-sensitive brand match left
/// the Xeon decoder unrun on Emerald Rapids from the day it was written.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Lz4Body {
    /// AArch64, or an x86-64 part without SSSE3.
    Baseline,
    /// x86-64 with SSSE3 and neither line's name.
    Ssse3,
    Xeon,
    Epyc,
    /// AArch64's Neoverse V2. The first part line on this architecture.
    NeoverseV2,
}

impl Lz4Body {
    /// Every body this *machine* can execute, so a test can reach all of them.
    ///
    /// Not every body this build contains: the two line bodies copy 256 bits at
    /// a time, and calling one directly on a part without AVX is an illegal
    /// instruction rather than a failed assertion. Dispatch would never pick
    /// them there, and neither does this.
    pub fn all() -> &'static [Lz4Body] {
        #[cfg(all(lzvolt_asm, target_arch = "x86_64"))]
        {
            if crate::cpu::features().avx2 {
                &[
                    Lz4Body::Baseline,
                    Lz4Body::Ssse3,
                    Lz4Body::Xeon,
                    Lz4Body::Epyc,
                ]
            } else {
                &[Lz4Body::Baseline, Lz4Body::Ssse3]
            }
        }
        // On AArch64 both bodies run anywhere: the part line changes the shape
        // of a loop, not the instructions available, so the soak reaches the
        // Neoverse body from an Apple host. That matters -- the x86 line bodies
        // went unexercised on this machine for as long as `all()` withheld them,
        // and a kernel nobody runs is a kernel nobody tests.
        #[cfg(all(lzvolt_asm, target_arch = "aarch64"))]
        {
            &[Lz4Body::Baseline, Lz4Body::NeoverseV2]
        }
        #[cfg(not(all(lzvolt_asm, any(target_arch = "x86_64", target_arch = "aarch64"))))]
        {
            &[Lz4Body::Baseline]
        }
    }
}

/// The body this machine gets, from the cached feature set.
#[cfg(all(lzvolt_asm, any(target_arch = "aarch64", target_arch = "x86_64")))]
#[inline]
fn detect_lz4_body() -> Lz4Body {
    #[cfg(target_arch = "x86_64")]
    {
        let f = crate::cpu::features();
        if !f.ssse3 {
            return Lz4Body::Baseline;
        }
        // The two line bodies copy the final literal run 256 bits at a time,
        // which every Xeon and EPYC since 2011 can do -- but that is an
        // assumption about a brand string, and a brand string is not a
        // capability. CPUID is asked as well, and a part that says no gets the
        // SSSE3 body, which assumes nothing.
        if f.avx2 {
            if f.xeon {
                return Lz4Body::Xeon;
            }
            if f.epyc {
                return Lz4Body::Epyc;
            }
        }
        Lz4Body::Ssse3
    }
    #[cfg(target_arch = "aarch64")]
    {
        if crate::cpu::features().neoverse_v2 {
            return Lz4Body::NeoverseV2;
        }
        Lz4Body::Baseline
    }
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    {
        Lz4Body::Baseline
    }
}

/// Which LZ4 body this machine resolves to.
///
/// For diagnostics: `examples/quick --entry` times the dispatch by calling
/// [`unpack_lz4_into_slice_on`] with the answer hoisted out of the loop and
/// comparing that against [`unpack_lz4_into_slice`], which asks again per call.
pub fn lz4_body() -> Lz4Body {
    #[cfg(all(lzvolt_asm, any(target_arch = "aarch64", target_arch = "x86_64")))]
    {
        detect_lz4_body()
    }
    #[cfg(not(all(lzvolt_asm, any(target_arch = "aarch64", target_arch = "x86_64"))))]
    {
        Lz4Body::Baseline
    }
}

/// Run the named LZ4 body.
///
/// A second function rather than a fourth arm in [`run`]: the two formats pick
/// from different sets -- there is no wide LZ4 -- and keeping them apart means
/// our own format's dispatch is not touched when this one grows.
///
/// # Safety
///
/// As [`run`]: `dst_cap` bytes writable at `dst`, `src_len` readable at `src`.
#[cfg(all(lzvolt_asm, any(target_arch = "aarch64", target_arch = "x86_64")))]
#[inline]
unsafe fn run_lz4(
    body: Lz4Body,
    src: *const u8,
    src_len: usize,
    dst: *mut u8,
    dst_cap: usize,
    declared: usize,
    start: usize,
) -> u32 {
    #[cfg(target_arch = "x86_64")]
    match body {
        Lz4Body::Xeon => return keva_unpack_lz4_xeon(src, src_len, dst, dst_cap, declared, start),
        Lz4Body::Epyc => return keva_unpack_lz4_epyc(src, src_len, dst, dst_cap, declared, start),
        Lz4Body::Ssse3 => {
            return keva_unpack_lz4_ssse3(src, src_len, dst, dst_cap, declared, start)
        }
        Lz4Body::Baseline | Lz4Body::NeoverseV2 => {}
    }
    #[cfg(target_arch = "aarch64")]
    if body == Lz4Body::NeoverseV2 {
        return keva_unpack_lz4_neoverse(src, src_len, dst, dst_cap, declared, start);
    }
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    let _ = body;

    keva_unpack_lz4(src, src_len, dst, dst_cap, declared, start)
}

/// [`unpack_lz4_into_slice`] against a named body rather than this machine's.
///
/// For the differential test and nothing else: a body only ever runs on the
/// part it was built for.
pub fn unpack_lz4_into_slice_on(
    body: Lz4Body,
    src: &[u8],
    dst: &mut [u8],
    declared: usize,
) -> bool {
    #[cfg(not(all(lzvolt_asm, any(target_arch = "aarch64", target_arch = "x86_64"))))]
    {
        let _ = (body, src, dst, declared);
        false
    }

    #[cfg(all(lzvolt_asm, any(target_arch = "aarch64", target_arch = "x86_64")))]
    {
        if declared == 0 {
            return false;
        }
        // SAFETY: `dst.len()` is the true length of the buffer, and the kernel
        // writes below it or returns zero.
        let produced = unsafe {
            run_lz4(
                body,
                src.as_ptr(),
                src.len(),
                dst.as_mut_ptr(),
                dst.len(),
                declared,
                0,
            )
        } as usize;
        produced == declared
    }
}

/// Decode a block written by liblz4 or any other LZ4 implementation.
///
/// The counterpart of `LZ4_decompress_safe`, and the only entry that reaches the
/// LZ4 body. The separation is the point: what makes foreign blocks faster must
/// not be able to change the timing of our own values, and a shared body cannot
/// promise that.
///
/// `declared` is the uncompressed length, which the caller must supply because
/// **an LZ4 block does not carry one**. That is also how the two formats are
/// told apart without a heuristic: ours has its length in a varint at the
/// front, so its caller does not pass one, and this one's caller must.
///
/// `dst` must be at least `declared + UNPACK_SLACK` long; the kernel refuses
/// otherwise. Returns whether it decoded.
pub fn unpack_lz4_into_slice(src: &[u8], dst: &mut [u8], declared: usize) -> bool {
    #[cfg(not(all(lzvolt_asm, any(target_arch = "aarch64", target_arch = "x86_64"))))]
    {
        let _ = (src, dst, declared);
        false
    }

    #[cfg(all(lzvolt_asm, any(target_arch = "aarch64", target_arch = "x86_64")))]
    {
        if declared == 0 {
            return false;
        }
        // SAFETY: `dst.len()` is the true length of the buffer, and the kernel
        // writes below it or returns zero.
        unpack_lz4_into_slice_on(detect_lz4_body(), src, dst, declared)
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
/// the portable decoder, exactly as with `unpack_asm`.
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
/// `unpack_asm` use.
pub fn unpack_section(
    src: &[u8],
    dst: &mut [u8],
    declared: usize,
    start: usize,
    split: Split,
) -> bool {
    #[cfg(not(all(lzvolt_asm, any(target_arch = "aarch64", target_arch = "x86_64"))))]
    {
        let _ = (src, dst, declared, start, split);
        false
    }

    #[cfg(all(lzvolt_asm, any(target_arch = "aarch64", target_arch = "x86_64")))]
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
/// as with `unpack_asm`.
pub fn unpack_asm_hybrid(
    body: &[u8],
    out: &mut Vec<u8>,
    declared: usize,
    switch: (usize, usize),
    first: Split,
    second: Split,
) -> bool {
    #[cfg(not(all(lzvolt_asm, any(target_arch = "aarch64", target_arch = "x86_64"))))]
    {
        let _ = (body, out, declared, switch, first, second);
        false
    }

    #[cfg(all(lzvolt_asm, any(target_arch = "aarch64", target_arch = "x86_64")))]
    {
        out.clear();
        // `Vec::reserve` is a call whose body handles growth, and the caller
        // reuses its buffer, so growth is the case that never happens. Asking
        // about the capacity first keeps the common path to a compare.
        let want = declared + UNPACK_SLACK;
        if out.capacity() < want {
            out.reserve(want);
        }
        let cap = out.capacity();
        // SAFETY: `cap` is this allocation's real capacity, so the slice
        // covers memory we own. None of it is read as initialised: the length
        // is set only once, below, and only to what the kernels wrote.
        let dst = unsafe { std::slice::from_raw_parts_mut(out.as_mut_ptr(), cap) };
        if !unpack_hybrid_into_slice(body, dst, declared, switch, first, second) {
            out.clear();
            return false;
        }
        // SAFETY: both sections reported reaching their targets, so every byte
        // below `declared` was written by one of them.
        unsafe { out.set_len(declared) };
        true
    }
}

/// The same two calls, writing into a buffer the caller already owns.
///
/// This is the shape a C caller needs, and [`unpack_asm_hybrid`] is written
/// over it: a `Vec` is a buffer plus a length, and the length is the only part
/// the kernels do not deal in.
///
/// `dst` **should** be at least `declared + UNPACK_SLACK` so the kernels can
/// take their wide paths. A shorter buffer is not unsafe — each kernel
/// re-checks the bound it was handed — it only declines more often, and a
/// decline means the portable decoder runs instead.
pub fn unpack_hybrid_into_slice(
    body: &[u8],
    dst: &mut [u8],
    declared: usize,
    switch: (usize, usize),
    first: Split,
    second: Split,
) -> bool {
    #[cfg(not(all(lzvolt_asm, any(target_arch = "aarch64", target_arch = "x86_64"))))]
    {
        let _ = (body, dst, declared, switch, first, second);
        false
    }

    #[cfg(all(lzvolt_asm, any(target_arch = "aarch64", target_arch = "x86_64")))]
    {
        let (in_at, out_at) = switch;
        if declared == 0 || out_at == 0 || out_at >= declared || in_at > body.len() {
            return false;
        }
        if dst.len() < declared {
            return false;
        }

        // SAFETY: `dst.len()` is the true length of the buffer. The first call
        // is given `out_at` as its declared length and the second starts there
        // and stops at `declared`; both are at most `dst.len()`, and each
        // kernel checks that bound before taking any path that writes wide.
        unsafe {
            let cap = dst.len();
            let base = dst.as_mut_ptr();
            let first_len = run(first, body.as_ptr(), in_at, base, cap, out_at, 0) as usize;
            if first_len != out_at {
                return false;
            }
            let tail = body.as_ptr().add(in_at);
            let tail_len = body.len() - in_at;
            let total = run(second, tail, tail_len, base, cap, declared, out_at) as usize;
            total == declared
        }
    }
}

/// Whether this build has an assembly decoder.
pub const fn asm_available() -> bool {
    cfg!(all(
        lzvolt_asm,
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
    #[cfg(not(all(lzvolt_asm, any(target_arch = "aarch64", target_arch = "x86_64"))))]
    {
        let _ = (body, out, declared, split);
        false
    }

    #[cfg(all(lzvolt_asm, any(target_arch = "aarch64", target_arch = "x86_64")))]
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

/// `unpack_asm` for a block somebody else wrote.
///
/// Same buffer handling, the LZ4 body instead of ours. See
/// [`unpack_lz4_into_slice`] for why the two are separate symbols and how the
/// formats are told apart.
pub fn unpack_lz4_asm(block: &[u8], out: &mut Vec<u8>, declared: usize) -> bool {
    #[cfg(not(all(lzvolt_asm, any(target_arch = "aarch64", target_arch = "x86_64"))))]
    {
        let _ = (block, out, declared);
        false
    }

    #[cfg(all(lzvolt_asm, any(target_arch = "aarch64", target_arch = "x86_64")))]
    {
        if declared == 0 {
            return false;
        }

        out.clear();
        let want = declared + UNPACK_SLACK;
        if out.capacity() < want {
            out.reserve(want);
        }
        let cap = out.capacity();

        // SAFETY: as `unpack_asm` -- `cap` is the real capacity, the kernel
        // writes below `declared + 64` or returns zero, and the length only
        // moves out to `declared`.
        let produced = unsafe {
            run_lz4(
                detect_lz4_body(),
                block.as_ptr(),
                block.len(),
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

        // SAFETY: the kernel reported writing exactly `declared` bytes.
        unsafe { out.set_len(declared) };
        true
    }
}
