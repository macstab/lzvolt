//! The packer's match search, in assembly where one exists.
//!
//! This is the loop every profile sample lands in: per position it hashes four
//! bytes, reads and rewrites one table slot, and compares four bytes. The
//! comparison rejects almost every position, so the ordering matters more than
//! the arithmetic.
//!
//! # Why this one and not the whole packer
//!
//! It is the part with a shape worth writing by hand — a tight loop over a
//! fixed register set — and it has a boundary a caller can hold: a block, two
//! positions and a table. Emitting the blocks around it is bookkeeping, not
//! throughput, and putting it in assembly would double the surface for no
//! measurable return.
//!
//! # The rule this crate holds every kernel to
//!
//! A backend that cannot be diffed against a reference is a backend nobody can
//! trust. [`find_scalar`] is that reference, and the differential test drives
//! both over the same inputs and requires identical results — not merely a
//! valid match, but the *same* match, since a different one would change what
//! the packer emits and silently alter the compressed form.

/// Where the search got to, and what it found.
///
/// Laid out for the assembly: four `u32` in this order, so the kernel reads and
/// writes fixed offsets. Changing the order without changing the `.S` files
/// breaks it silently, which is what the differential test is for.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PackState {
    /// In: where to resume. Out: where the match sits, or where the scan ended.
    pub at: u32,
    /// Out: the earlier position the match refers back to.
    pub candidate: u32,
    /// Out: how long the match is. Zero means the scan ran out first.
    pub len: u32,
    /// In and out: the skip accelerator's counter. Carried across calls so the
    /// search keeps widening its stride through a stretch that matches nothing.
    pub misses: u32,
}

/// Shortest run worth encoding. Must agree with `keva_core::store::pack`.
pub const MIN_MATCH: usize = 4;
/// Longest backward reference. Must agree with `keva_core::store::pack`.
pub const MAX_OFFSET: usize = 65_535;
/// Table size as a power of two, and how much of it a small value uses. Must
/// agree with `keva_core::store::pack`.
pub const HASH_BITS: u32 = 12;
/// How much of the table a value above [`NARROW_TABLE_ABOVE`] uses. The table is
/// still allocated in full; a large value simply touches less of it, and touched
/// lines are what the cache charges for.
pub const HASH_BITS_LARGE: u32 = 11;
/// Length above which the packer narrows the table. Must agree with
/// `keva_core::store::pack`.
pub const NARROW_TABLE_ABOVE: usize = 8192;
/// Misses before the stride widens. Must agree with `keva_core::store::pack`.
pub const SKIP_TRIGGER: u32 = 6;

/// Entries the table must hold.
pub const TABLE_SIZE: usize = 1 << HASH_BITS;

/// A slot that has never been written. Must agree with `keva_core::store::pack`.
///
/// Not zero, and that is load-bearing rather than stylistic. The kernel decides
/// whether a candidate is usable with a single unsigned compare of `at -
/// stored` against the window; a sentinel large enough to make that subtraction
/// wrap fails the compare already, which is how the "is this slot empty" branch
/// was removed from the hot loop entirely.
///
/// The cost is that a zero-filled table is not merely unhelpful, it is unsound:
/// slot zero would read as position -1 and send the verify load outside the
/// buffer. Build tables with [`new_table`].
pub const EMPTY: u32 = 0x8000_0000;

/// A match table, initialised the only way the kernels accept.
///
/// This exists so that no caller has to know about [`EMPTY`]. Handing
/// [`pack_asm`] a `vec![0u32; TABLE_SIZE]` would read out of bounds, and a
/// constructor is a cheaper defence than a comment.
pub fn new_table() -> Vec<u32> {
    vec![EMPTY; TABLE_SIZE]
}

#[cfg(all(lzv_asm, target_arch = "aarch64"))]
extern "C" {
    fn keva_pack_find(input: *const u8, len: usize, table: *mut u32, state: *mut PackState);
}

#[cfg(all(lzv_asm, any(target_arch = "aarch64", target_arch = "x86_64")))]
extern "C" {
    fn keva_pack(
        src: *const u8,
        src_len: usize,
        dst: *mut u8,
        dst_cap: usize,
        table: *mut u32,
    ) -> u32;
}

// One symbol per part line. Declared only where they exist: nothing outside
// x86-64 links them.
#[cfg(all(lzv_asm, target_arch = "x86_64"))]
extern "C" {
    fn keva_pack_xeon(
        src: *const u8,
        src_len: usize,
        dst: *mut u8,
        dst_cap: usize,
        table: *mut u32,
    ) -> u32;

    /// AMD server line. Separate from the Xeon body for the same reason the
    /// decoder's are: the same code has measured opposite signs on the two.
    fn keva_pack_amd(
        src: *const u8,
        src_len: usize,
        dst: *mut u8,
        dst_cap: usize,
        table: *mut u32,
    ) -> u32;
}

/// Which assembled body a call runs.
///
/// The three are identical today and the byte-for-byte test requires them to
/// stay that way; the split exists so one can be tuned without disturbing
/// another. AMD is first place on every shape measured, so it is the one that
/// does not move.
///
/// Vendor rather than the decoder's finer part-line split, and that is what the
/// evidence supports: Xeon and EPYC have been measured, no Core part has.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PartLine {
    /// A Core part, or a vendor nobody has measured.
    Baseline,
    Xeon,
    Amd,
}

impl PartLine {
    /// Every line this build has a body for, so a test can reach all of them.
    ///
    /// Dispatch means only one runs on a given machine, so without this an
    /// Intel runner would never execute the AMD body and a divergence in it
    /// could sit unnoticed until someone ran it in production.
    pub fn all() -> &'static [PartLine] {
        #[cfg(all(lzv_asm, target_arch = "x86_64"))]
        {
            &[PartLine::Baseline, PartLine::Xeon, PartLine::Amd]
        }
        #[cfg(not(all(lzv_asm, target_arch = "x86_64")))]
        {
            &[PartLine::Baseline]
        }
    }
}

/// The line this machine is, decided from the cached feature set.
///
/// `cpu::features()` is a `OnceLock`, so CPUID runs once per process and this
/// is a predicted branch on a value that never changes -- once per record,
/// outside the kernel. Inside it there is no CPU check at all.
#[cfg(all(lzv_asm, target_arch = "x86_64"))]
#[inline]
fn detect_line() -> PartLine {
    let f = crate::cpu::features();
    if f.xeon {
        PartLine::Xeon
    } else if f.amd {
        PartLine::Amd
    } else {
        PartLine::Baseline
    }
}

/// # Safety
///
/// The caller owes what the kernel's contract asks: `dst_cap` bytes writable at
/// `dst`, `src_len` readable at `src`, and a table of at least [`TABLE_SIZE`].
#[cfg(all(lzv_asm, any(target_arch = "aarch64", target_arch = "x86_64")))]
#[inline]
unsafe fn run(
    line: PartLine,
    src: *const u8,
    src_len: usize,
    dst: *mut u8,
    dst_cap: usize,
    table: *mut u32,
) -> u32 {
    // The `return` is not needless, whatever clippy sees on the target where
    // the other arm is compiled out: this is a statement block and the tail
    // belongs to the other `cfg`.
    #[cfg(target_arch = "x86_64")]
    #[allow(clippy::needless_return)]
    {
        return match line {
            PartLine::Xeon => keva_pack_xeon(src, src_len, dst, dst_cap, table),
            PartLine::Amd => keva_pack_amd(src, src_len, dst, dst_cap, table),
            PartLine::Baseline => keva_pack(src, src_len, dst, dst_cap, table),
        };
    }

    #[cfg(not(target_arch = "x86_64"))]
    {
        let _ = line;
        keva_pack(src, src_len, dst, dst_cap, table)
    }
}

/// Output capacity the x86-64 kernel is given, and what it checks for.
///
/// The Rust packer's own worst case, `Cursor::room`: the whole input as
/// literals, a token and an offset for every block, the chains of 255s, and the
/// overshoot the fixed-width literal move makes. Reserving it up front is what
/// lets the block writer carry no bounds checks -- the same argument that makes
/// the Rust packer sound, made once here instead of per store.
///
/// The AArch64 kernel is not sized this way. It bails out the moment its output
/// reaches the input length, so `src_len + 16` is sufficient there, and it is
/// left alone: changing what that kernel is handed is a separate measurement.
#[cfg(all(lzv_asm, target_arch = "x86_64"))]
fn room(n: usize) -> usize {
    n + n / MIN_MATCH * 3 + n / 128 + 16 + 64 + 10
}

/// Pack `input` into `out` in one call.
///
/// Returns the number of bytes written, or `None` when packing did not help
/// and the caller should store the original.
///
/// One call per value, deliberately. The search alone was measured 4-17% ahead
/// of the compiler's version, and then made the packer 16-29% *slower* when
/// called once per match: an `extern "C"` boundary costs a few cycles, but it
/// stops the compiler carrying loop state in registers, and crossing it that
/// often costs more than the kernel wins. Crossing once per value is what makes
/// the difference reach the caller.
///
/// `out` is given `input.len() + 16` bytes of capacity, which is what the kernel
/// needs to decide it has failed without bounds-checking every store — it stops
/// as soon as the output reaches the input length, since packing has not helped
/// by then anyway.
///
/// The buffer is zero-filled first, and that turns out to be worth its cost. On
/// AArch64 a memset clears whole cache lines with `dc zva`, which takes
/// ownership of a line *without reading it*; skipping it means every first store
/// into a line pays a read-for-ownership instead. Reserving without zeroing was
/// measured and lost 12% on a 64 KiB value that compresses to 33 KiB, while
/// gaining 1-3% on values whose output is a few hundred bytes — so the memset is
/// a prefetch in disguise, and it pays wherever there is real output to write.
///
/// `table` must have come from [`new_table`], or from an earlier call to this
/// function. A zero-filled one is unsound -- see [`EMPTY`].
/// The kernel writes the even split only. A value whose matches keep
/// overflowing four bits is packed by the portable packer instead, which is
/// where the choice between the two splits is made.
pub fn pack_asm(input: &[u8], out: &mut Vec<u8>, table: &mut [u32]) -> Option<usize> {
    // As above: the tail of this function belongs to the other `cfg`.
    #[cfg(all(lzv_asm, target_arch = "x86_64"))]
    #[allow(clippy::needless_return)]
    {
        return pack_asm_on(detect_line(), input, out, table);
    }
    #[cfg(not(all(lzv_asm, target_arch = "x86_64")))]
    #[allow(clippy::needless_return)]
    {
        pack_asm_on(PartLine::Baseline, input, out, table)
    }
}

/// Pack through one named body, whichever machine this is.
///
/// Production goes through [`pack_asm`], which picks the line. This exists so
/// the differential test can drive every assembled body rather than only the
/// one the runner happens to be: an Intel machine would otherwise never
/// execute the AMD body, and a divergence there would wait for production to
/// find it.
pub fn pack_asm_on(
    line: PartLine,
    input: &[u8],
    out: &mut Vec<u8>,
    table: &mut [u32],
) -> Option<usize> {
    debug_assert!(table.len() >= TABLE_SIZE);
    let _ = line;

    // The `return` is not needless, whatever clippy sees: this is a statement
    // block, not the function's tail, and the tail belongs to the other `cfg`.
    // Dropping it would evaluate `None` and discard it.
    #[cfg(not(all(
        lzv_asm,
        any(target_arch = "aarch64", target_arch = "x86_64")
    )))]
    #[allow(clippy::needless_return)]
    {
        let _ = (input, out, table);
        return None;
    }

    // x86-64 gets the reserve without the zero-fill, and that is a difference
    // between the architectures rather than an oversight. The AArch64 memset
    // pays for itself because `dc zva` takes ownership of a cache line without
    // reading it, so it removes a read-for-ownership from every first store
    // into a line -- worth 12% on a 64 KiB value there. x86-64 has no such
    // instruction, so the argument does not carry over and the fill would be
    // pure cost. Whether `rep stosb` reaches the same place by another route is
    // a question for the GCE run, not an assumption to build in.
    #[cfg(all(lzv_asm, target_arch = "x86_64"))]
    #[allow(clippy::needless_return)]
    {
        out.clear();
        let want = room(input.len());
        if out.capacity() < want {
            out.reserve(want);
        }
        let cap = out.capacity();

        // SAFETY: `cap` is the real capacity, so the whole range handed to the
        // kernel is allocated, and the kernel refuses outright unless `cap`
        // covers its worst case. `table` is at least `TABLE_SIZE`, which covers
        // every index the hash can produce.
        let written = unsafe {
            run(
                line,
                input.as_ptr(),
                input.len(),
                out.as_mut_ptr(),
                cap,
                table.as_mut_ptr(),
            )
        } as usize;

        if written == 0 {
            return None;
        }
        assert!(written <= cap, "the kernel reported writing past the buffer");
        // SAFETY: the kernel wrote `written` bytes from the pointer, and
        // `written <= cap` was just checked.
        unsafe { out.set_len(written) };
        return Some(written);
    }

    #[cfg(all(lzv_asm, target_arch = "aarch64"))]
    {
        out.clear();
        out.resize(input.len() + 16, 0);
        let cap = out.len();

        // SAFETY: `cap` is the real capacity, so the whole range the kernel is
        // handed is allocated, and the kernel is documented to write below it or
        // return zero. `table` is at least `TABLE_SIZE`, which covers every
        // index the hash can produce. Nothing reads the spare capacity: the
        // length only moves out to `written`, and every byte below it was
        // written by this call.
        let written = unsafe {
            run(
                line,
                input.as_ptr(),
                input.len(),
                out.as_mut_ptr(),
                cap,
                table.as_mut_ptr(),
            )
        } as usize;

        if written == 0 {
            out.clear();
            return None;
        }
        assert!(
            written <= cap,
            "the kernel reported writing past the buffer"
        );
        // SAFETY: the kernel wrote `written` bytes starting at the pointer, and
        // `written <= cap` was just checked.
        unsafe { out.set_len(written) };
        Some(written)
    }
}

/// Whether this build has an assembly kernel for packing.
pub const fn asm_available() -> bool {
    cfg!(all(
        lzv_asm,
        any(target_arch = "aarch64", target_arch = "x86_64")
    ))
}

/// Which portable packer this target's kernel is the counterpart of.
///
/// The two kernels do not implement the same packer, and pretending otherwise
/// would make the differential test pass by comparing the wrong things. AArch64
/// implements the eager, fixed-split search and is diffed against the reference
/// that matches it. x86-64 implements the production pass -- lazy matching, the
/// backward extension and the adaptive split -- so it is diffed against the
/// packer that actually runs.
pub const fn kernel_is_production_packer() -> bool {
    cfg!(all(lzv_asm, target_arch = "x86_64"))
}

/// Advance the search until it finds a match or runs out of input.
///
/// `table` must hold at least [`TABLE_SIZE`] entries. It is not cleared between
/// calls: a stale entry is checked against the cursor and then verified byte
/// for byte, so it can only cost a rejected candidate, never a wrong match.
pub fn find(input: &[u8], table: &mut [u32], state: &mut PackState) {
    debug_assert!(table.len() >= TABLE_SIZE);

    #[cfg(all(lzv_asm, target_arch = "aarch64"))]
    // SAFETY: the kernel reads `input` only at offsets it has bounds-checked
    // against `len`, and touches `table` only at indices masked to HASH_BITS,
    // which `TABLE_SIZE` covers. `state` is a `#[repr(C)]` struct of four
    // `u32`, which is the layout the `.S` file reads and writes.
    unsafe {
        keva_pack_find(input.as_ptr(), input.len(), table.as_mut_ptr(), state);
    }

    #[cfg(not(all(lzv_asm, target_arch = "aarch64")))]
    find_scalar(input, table, state);
}

/// The portable reference, and what the assembly is diffed against.
///
/// Written to be read rather than to be fast: every kernel in this crate has to
/// be checkable against something obviously correct, and this is that something.
pub fn find_scalar(input: &[u8], table: &mut [u32], state: &mut PackState) {
    let mut at = state.at as usize;
    let mut misses = state.misses;

    while at + MIN_MATCH <= input.len() {
        let word = u32::from_le_bytes(input[at..at + 4].try_into().unwrap());
        let slot = (word.wrapping_mul(0x9E37_79B1) >> (32 - HASH_BITS)) as usize;

        let stored = table[slot];
        table[slot] = at as u32 + 1;

        if stored != 0 {
            let candidate = stored as usize - 1;
            if candidate < at
                && at - candidate <= MAX_OFFSET
                && u32::from_le_bytes(input[candidate..candidate + 4].try_into().unwrap()) == word
            {
                let mut matched = MIN_MATCH;
                let mut a = candidate + MIN_MATCH;
                let mut b = at + MIN_MATCH;
                while b + 8 <= input.len() {
                    let left = u64::from_le_bytes(input[a..a + 8].try_into().unwrap());
                    let right = u64::from_le_bytes(input[b..b + 8].try_into().unwrap());
                    let diff = left ^ right;
                    if diff != 0 {
                        matched += (diff.trailing_zeros() / 8) as usize;
                        state.at = at as u32;
                        state.candidate = candidate as u32;
                        state.len = matched as u32;
                        state.misses = 1 << SKIP_TRIGGER;
                        return;
                    }
                    a += 8;
                    b += 8;
                    matched += 8;
                }
                while b < input.len() && input[a] == input[b] {
                    a += 1;
                    b += 1;
                    matched += 1;
                }
                state.at = at as u32;
                state.candidate = candidate as u32;
                state.len = matched as u32;
                state.misses = 1 << SKIP_TRIGGER;
                return;
            }
        }

        at += (misses >> SKIP_TRIGGER) as usize;
        misses += 1;
    }

    state.at = at as u32;
    state.len = 0;
    state.misses = misses;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Records-shaped JSON: a few varying fields in a fixed frame, which is
    /// what reaches the wide split and what a cache mostly holds.
    fn records(total: usize) -> Vec<u8> {
        let mut out = Vec::with_capacity(total);
        let mut i = 0u64;
        while out.len() < total {
            out.extend_from_slice(
                format!("{{\"id\":{i},\"tenant\":\"t42\",\"active\":true}}").as_bytes(),
            );
            i += 1;
        }
        out.truncate(total);
        out
    }

    /// The same frame with high-entropy fields, so matches stay short and the
    /// block rate rather than the copy rate decides the cost.
    fn varied(total: usize) -> Vec<u8> {
        let mut state = 0x2545_F491_4F6C_DD1Du64;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        let mut out = Vec::with_capacity(total);
        while out.len() < total {
            out.extend_from_slice(format!("{{\"k\":\"{:016x}\"}}", next()).as_bytes());
        }
        out.truncate(total);
        out
    }

    /// Incompressible. The packer declines to pack it, so this exercises the
    /// refusal path rather than the copy.
    fn noise(total: usize) -> Vec<u8> {
        let mut state = 0x9E37_79B9_7F4A_7C15u64;
        (0..total)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                state as u8
            })
            .collect()
    }

    /// Drive both implementations over the whole of an input and require that
    /// every step agrees.
    ///
    /// Not "both found a valid match" — the *same* match. A different one is
    /// still correct output from the packer's point of view and would change
    /// the bytes it emits, which is exactly the kind of divergence that shows
    /// up months later as two nodes disagreeing about a snapshot.
    fn agree(input: &[u8]) {
        let mut asm_table = vec![0u32; TABLE_SIZE];
        let mut ref_table = vec![0u32; TABLE_SIZE];
        let mut asm_state = PackState {
            misses: 1 << SKIP_TRIGGER,
            ..Default::default()
        };
        let mut ref_state = asm_state;

        let mut steps = 0usize;
        loop {
            find(input, &mut asm_table, &mut asm_state);
            find_scalar(input, &mut ref_table, &mut ref_state);

            assert_eq!(
                asm_state,
                ref_state,
                "step {steps} disagreed on a {}-byte input",
                input.len()
            );

            if asm_state.len == 0 {
                break;
            }

            // Both advance the way the packer would.
            let next = asm_state.at + asm_state.len;
            asm_state.at = next;
            ref_state.at = next;
            steps += 1;
            assert!(steps < input.len() + 8, "search failed to terminate");
        }
    }

    #[test]
    fn agrees_with_the_reference_on_real_shapes() {
        for size in [0usize, 1, 3, 4, 5, 7, 8, 16, 64, 1000, 4096, 65_536] {
            agree(&records(size));
            agree(&varied(size));
            agree(&noise(size));
        }
    }

    #[test]
    fn agrees_on_degenerate_input() {
        agree(&vec![b'a'; 4096]);
        agree(&vec![0u8; 4096]);
        agree(&[b'a', b'b'].repeat(2048));
        // A single long run preceded and followed by something else, which is
        // where the match extension has to stop in the right place.
        let mut edge = b"prefix".to_vec();
        edge.extend(std::iter::repeat(b'z').take(3000));
        edge.extend_from_slice(b"suffix");
        agree(&edge);
    }

    /// The constants live in two places because a `.S` file cannot read Rust.
    /// If they drift, matches diverge silently; this is the tripwire.
    #[test]
    fn the_duplicated_constants_still_hold() {
        assert_eq!(TABLE_SIZE, 4096);
        assert_eq!(32 - HASH_BITS, 20, "the small-value shift in the .S file");
        assert_eq!(
            32 - HASH_BITS_LARGE,
            21,
            "the large-value shift in the .S file"
        );
        assert_eq!(NARROW_TABLE_ABOVE, 8192, "the threshold in the .S file");
        assert_eq!(EMPTY, 0x8000_0000, "the sentinel the .S file relies on");
        assert_eq!(MIN_MATCH, 4);
        assert_eq!(MAX_OFFSET, 65_535);
        assert_eq!(1u32 << SKIP_TRIGGER, 64, "the reset value in the .S file");
        assert_eq!(std::mem::size_of::<PackState>(), 16);
    }
}
