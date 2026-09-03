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
/// Table size as a power of two. Must agree with `keva_core::store::pack`.
pub const HASH_BITS: u32 = 12;
/// Misses before the stride widens. Must agree with `keva_core::store::pack`.
pub const SKIP_TRIGGER: u32 = 6;

/// Entries the table must hold.
pub const TABLE_SIZE: usize = 1 << HASH_BITS;


#[cfg(all(keva_asm, target_arch = "aarch64"))]
extern "C" {
    fn keva_pack_find(input: *const u8, len: usize, table: *mut u32, state: *mut PackState);

    fn keva_pack(
        src: *const u8,
        src_len: usize,
        dst: *mut u8,
        dst_cap: usize,
        table: *mut u32,
    ) -> u32;
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
pub fn pack_asm(input: &[u8], out: &mut Vec<u8>, table: &mut [u32]) -> Option<usize> {
    debug_assert!(table.len() >= TABLE_SIZE);

    #[cfg(not(all(keva_asm, target_arch = "aarch64")))]
    {
        let _ = (input, out, table);
        return None;
    }

    #[cfg(all(keva_asm, target_arch = "aarch64"))]
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
            keva_pack(
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
        assert!(written <= cap, "the kernel reported writing past the buffer");
        // SAFETY: the kernel wrote `written` bytes starting at the pointer, and
        // `written <= cap` was just checked.
        unsafe { out.set_len(written) };
        Some(written)
    }
}

/// Whether this build has an assembly kernel for the search.
pub const fn asm_available() -> bool {
    cfg!(all(keva_asm, target_arch = "aarch64"))
}

/// Advance the search until it finds a match or runs out of input.
///
/// `table` must hold at least [`TABLE_SIZE`] entries. It is not cleared between
/// calls: a stale entry is checked against the cursor and then verified byte
/// for byte, so it can only cost a rejected candidate, never a wrong match.
pub fn find(input: &[u8], table: &mut [u32], state: &mut PackState) {
    debug_assert!(table.len() >= TABLE_SIZE);

    #[cfg(all(keva_asm, target_arch = "aarch64"))]
    // SAFETY: the kernel reads `input` only at offsets it has bounds-checked
    // against `len`, and touches `table` only at indices masked to HASH_BITS,
    // which `TABLE_SIZE` covers. `state` is a `#[repr(C)]` struct of four
    // `u32`, which is the layout the `.S` file reads and writes.
    unsafe {
        keva_pack_find(input.as_ptr(), input.len(), table.as_mut_ptr(), state);
    }

    #[cfg(not(all(keva_asm, target_arch = "aarch64")))]
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
        assert_eq!(32 - HASH_BITS, 20, "the shift baked into the .S file");
        assert_eq!(MIN_MATCH, 4);
        assert_eq!(MAX_OFFSET, 65_535);
        assert_eq!(1u32 << SKIP_TRIGGER, 64, "the reset value in the .S file");
        assert_eq!(std::mem::size_of::<PackState>(), 16);
    }
}
