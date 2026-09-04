//! Hand-written SIMD kernels for KevaVolt's hottest inner loops.
//!
//! # Why this crate exists
//!
//! Two claims are often confused, and only one of them is true here.
//!
//! *False*: "assembly makes a key-value store fast." It does not. The order of
//! magnitude comes from running commands on every core instead of one, from
//! `io_uring`, and from a memory layout that answers a probe out of a single
//! cache line. Those are architectural, and no amount of assembly substitutes
//! for them.
//!
//! *True*: once the working set is cache-resident, instruction count **is** the
//! runtime, and compilers leave measurable performance on the table there.
//! FFmpeg's own assembly documentation puts intrinsics at 10-15% behind
//! hand-written assembly, and dav1d measured 2x from auto-vectorisation against
//! 8x from hand-written kernels. The cost is real: register allocation and
//! instruction scheduling stay with the compiler when you use intrinsics, and
//! it will happily hoist a load out of a loop in a way that costs you a port.
//!
//! *Also true, and measured here*: that advantage is a function of kernel size,
//! not of assembly as such. Below roughly a dozen instructions, the fact that an
//! `extern "C"` kernel cannot be inlined costs more than better scheduling
//! returns. Our own control-group probe demonstrates it -- the hand-written NEON
//! version loses to the intrinsics by 47% purely on `call`/`ret`.
//!
//! So this crate is deliberately small, and a kernel earns the production path
//! only by beating its **intrinsics** version in `benches/` -- beating the
//! scalar loop merely shows that SIMD beats no SIMD. Assembly is reserved for
//! kernels long enough to amortise the call boundary: the expiry sweep,
//! eviction sampling, defragmentation. See
//! `docs/DESIGN_DECISION_ASM_KERNELS.md` for the admission criteria.
//!
//! # Correctness
//!
//! Every kernel has a scalar reference implementation in [`scalar`], and the
//! differential tests assert that all backends agree on exhaustive or randomly
//! generated inputs. The scalar path is a supported configuration, not a
//! fallback of last resort: building with `--no-default-features` disables the
//! assembly entirely and must still pass the full test suite.

#![cfg_attr(docsrs, feature(doc_cfg))]

// The three interchangeable kernel implementations live together in `backend`;
// `cpu` is the feature detection that would choose between wider ones.
pub mod backend;
pub mod cpu;
pub mod pack_find;
pub mod unpack;

pub use backend::{intrinsics, scalar};

/// Number of slots scanned by a single control-group probe.
///
/// This is 16 because that is one XMM register and one NEON `q` register, and
/// because 16 control bytes plus their group metadata fit a 64-byte cache line
/// with room for the probe sequence. The storage engine's group layout is built
/// around this constant; it is not freely tunable.
pub const GROUP_WIDTH: usize = 16;

/// A match result over one control group.
///
/// The bit representation is target-dependent by design. x86-64 gets a dense
/// mask from `pmovmskb`, one bit per slot. AArch64 has no equivalent
/// instruction, and the cheapest sequence there (`shrn`) produces four bits per
/// slot. Rather than pay extra instructions on ARM to force a common shape, the
/// stride travels with the mask and [`BitMask::iter`] divides it out. The
/// division is by a compile-time constant power of two, so it costs a shift.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct BitMask {
    bits: u64,
    stride: u32,
}

impl BitMask {
    #[inline(always)]
    fn new(bits: u64, stride: u32) -> Self {
        debug_assert!(stride.is_power_of_two());
        Self { bits, stride }
    }

    /// Wrap a raw mask produced by a backend other than the active one.
    ///
    /// Used by the differential tests and benchmarks, which have to construct
    /// masks from the intrinsics and scalar paths directly in order to compare
    /// them against the assembly kernel.
    #[inline(always)]
    pub fn from_raw(bits: u64, stride: u32) -> Self {
        Self::new(bits, stride)
    }

    /// True when no slot in the group matched.
    #[inline(always)]
    pub fn is_empty(self) -> bool {
        self.bits == 0
    }

    /// Index of the first matching slot, if any.
    #[inline(always)]
    pub fn lowest_slot(self) -> Option<usize> {
        if self.bits == 0 {
            None
        } else {
            Some(self.bits.trailing_zeros() as usize / self.stride as usize)
        }
    }

    /// Iterate the matching slot indices in ascending order.
    ///
    /// A probe usually stops at the first match, but tag collisions are
    /// expected -- with a 7-bit tag roughly one in 128 non-equal keys collides,
    /// so the caller must be able to continue after a failed full-key compare.
    #[inline(always)]
    pub fn iter(self) -> BitMaskIter {
        BitMaskIter(self)
    }

    /// The raw bits, for tests and benchmarks that compare backends directly.
    #[inline(always)]
    pub fn raw(self) -> u64 {
        self.bits
    }

    /// Bits occupied per slot on this target: 1 on x86-64, 4 on AArch64.
    #[inline(always)]
    pub fn stride(self) -> u32 {
        self.stride
    }

    /// Normalise to a dense one-bit-per-slot mask.
    ///
    /// Only for tests and diagnostics -- the hot path uses [`BitMask::iter`],
    /// which never materialises this form.
    pub fn to_dense(self) -> u16 {
        let mut dense = 0u16;
        for slot in self.iter() {
            dense |= 1 << slot;
        }
        dense
    }
}

impl core::fmt::Debug for BitMask {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "BitMask({:#018b}, stride={})",
            self.to_dense(),
            self.stride
        )
    }
}

/// Iterator over the matching slots of a [`BitMask`].
pub struct BitMaskIter(BitMask);

impl Iterator for BitMaskIter {
    type Item = usize;

    #[inline(always)]
    fn next(&mut self) -> Option<usize> {
        let slot = self.0.lowest_slot()?;
        // Clear every bit belonging to this slot, not just the lowest set one:
        // on AArch64 a matching slot sets all four of its bits.
        let slot_mask = match self.0.stride {
            1 => 1u64,
            n => (1u64 << n) - 1,
        } << (slot * self.0.stride as usize);
        self.0.bits &= !slot_mask;
        Some(slot)
    }
}

#[cfg(all(keva_asm, target_arch = "aarch64"))]
extern "C" {
    fn keva_ctrl_match_neon(ctrl: *const u8, tag: u8) -> u64;
}

#[cfg(all(keva_asm, target_arch = "x86_64"))]
extern "C" {
    fn keva_ctrl_match_sse2(ctrl: *const u8, tag: u8) -> u32;
}

/// Scan a 16-slot control group for `tag`.
///
/// This is the innermost operation of every `GET`, `SET` and `DEL`, so it is
/// the one kernel whose instruction count is worth arguing about.
///
/// **This uses intrinsics, not the assembly kernel, and that is a measured
/// decision.** On an M-series part the three paths come out at roughly 0.67 ns
/// for intrinsics, 1.01 ns for assembly and 2.75 ns for scalar. The assembly
/// loses to the intrinsics for a structural reason: an `extern "C"` kernel
/// cannot be inlined, and `call`/`ret` is about 1.3 cycles against a kernel
/// that is only about 2.7 cycles of actual work. Below roughly a dozen
/// instructions the call boundary costs more than better scheduling can
/// recover.
///
/// That is not an argument against hand-written assembly in general -- it is an
/// argument about kernel size. The streaming kernels (expiry sweep, eviction
/// sampling) run one call across millions of entries, the boundary amortises to
/// nothing, and there the FFmpeg-style 10-15% edge is expected to hold. See
/// `docs/DESIGN_DECISION_ASM_KERNELS.md`.
///
/// No runtime CPU dispatch happens here either: SSE2 is guaranteed on every
/// x86-64 part and NEON on every AArch64 part, so the path is fixed at compile
/// time with no branch in front of it. Dispatch begins to matter for the wide
/// kernels, which is what [`cpu`] is for.
#[inline(always)]
pub fn ctrl_match(ctrl: &[u8; GROUP_WIDTH], tag: u8) -> BitMask {
    match intrinsics::ctrl_match(ctrl, tag) {
        Some(bits) => BitMask::new(bits, intrinsics::stride()),
        None => BitMask::new(scalar::ctrl_match(ctrl, tag) as u64, 1),
    }
}

/// The hand-written assembly kernel, when this build has one.
///
/// Retained as the benchmark comparator rather than as the production path.
/// Keeping it compiled and tested is what makes the claim in [`ctrl_match`]
/// falsifiable: if a future compiler regresses, or someone finds a sequence
/// that beats the intrinsics by more than the call costs, the benchmark will
/// show it and the default can move back.
#[cfg(keva_asm)]
#[inline]
pub fn ctrl_match_asm(ctrl: &[u8; GROUP_WIDTH], tag: u8) -> Option<BitMask> {
    #[cfg(target_arch = "aarch64")]
    {
        // SAFETY: the kernel reads exactly 16 bytes from `ctrl`, which is a
        // reference to an array of exactly that length, and writes nothing.
        Some(BitMask::new(
            unsafe { keva_ctrl_match_neon(ctrl.as_ptr(), tag) },
            4,
        ))
    }

    #[cfg(target_arch = "x86_64")]
    {
        // SAFETY: as above -- a 16-byte read from a 16-byte array.
        Some(BitMask::new(
            unsafe { keva_ctrl_match_sse2(ctrl.as_ptr(), tag) } as u64,
            1,
        ))
    }

    #[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
    {
        let _ = (ctrl, tag);
        None
    }
}

/// Stub so callers and benchmarks compile identically when assembly is off.
#[cfg(not(keva_asm))]
#[inline]
pub fn ctrl_match_asm(_ctrl: &[u8; GROUP_WIDTH], _tag: u8) -> Option<BitMask> {
    None
}

/// Scan a control group for slots holding no live entry.
///
/// The encoding puts `FULL` in `0x00..=0x7F` and both `EMPTY` and `DELETED`
/// above `0x80`, so this is a single high-bit test rather than two comparisons —
/// on x86 literally one `pmovmskb`. Insertion needs it to find a landing slot,
/// and probing needs it to know when a probe sequence has hit an `EMPTY` and can
/// stop.
#[inline(always)]
pub fn ctrl_match_vacant(ctrl: &[u8; GROUP_WIDTH]) -> BitMask {
    match intrinsics::ctrl_match_vacant(ctrl) {
        Some(bits) => BitMask::new(bits, intrinsics::stride()),
        None => BitMask::new(scalar::ctrl_match_vacant(ctrl) as u64, 1),
    }
}

/// Portable reference form of [`ctrl_match_vacant`], for differential tests.
#[inline(always)]
pub fn ctrl_match_vacant_scalar(ctrl: &[u8; GROUP_WIDTH]) -> BitMask {
    BitMask::new(scalar::ctrl_match_vacant(ctrl) as u64, 1)
}

/// The portable reference implementation, always available.
///
/// Exposed so the differential tests and the benchmark baseline can call it
/// even on targets where an assembly kernel is active.
#[inline(always)]
pub fn ctrl_match_scalar(ctrl: &[u8; GROUP_WIDTH], tag: u8) -> BitMask {
    BitMask::new(scalar::ctrl_match(ctrl, tag) as u64, 1)
}

/// Whether this build assembled the hand-written kernels.
///
/// Note this reports availability as a *comparator*, not that [`ctrl_match`]
/// dispatches to assembly -- it currently does not, for the reasons documented
/// there.
pub const fn asm_enabled() -> bool {
    cfg!(all(
        keva_asm,
        any(target_arch = "aarch64", target_arch = "x86_64")
    ))
}

/// Human-readable name of the backend serving [`ctrl_match`], for `INFO`.
///
/// A benchmark number that cannot be traced back to the code path that produced
/// it is worthless, so this is reported alongside every result.
pub const fn backend_name() -> &'static str {
    if cfg!(target_arch = "aarch64") {
        "neon-intrinsics"
    } else if cfg!(target_arch = "x86_64") {
        "sse2-intrinsics"
    } else {
        "scalar"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Exhaustive over tags, randomised over group contents: every backend must
    /// agree with the scalar reference bit for bit.
    #[test]
    fn active_backend_matches_scalar_reference() {
        let mut state = 0x2545_F491_4F6C_DD1Du64;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };

        for _ in 0..10_000 {
            let mut ctrl = [0u8; GROUP_WIDTH];
            for chunk in ctrl.chunks_mut(8) {
                chunk.copy_from_slice(&next().to_le_bytes()[..chunk.len()]);
            }
            for tag in 0..=u8::MAX {
                let want = ctrl_match_scalar(&ctrl, tag);

                assert_eq!(
                    ctrl_match(&ctrl, tag).to_dense(),
                    want.to_dense(),
                    "backend {} disagreed on tag {tag:#04x} for {ctrl:?}",
                    backend_name()
                );

                // The assembly kernel is no longer the production path, but it
                // stays under test: an unverified comparator would make the
                // benchmark that demoted it meaningless.
                if let Some(from_asm) = ctrl_match_asm(&ctrl, tag) {
                    assert_eq!(
                        from_asm.to_dense(),
                        want.to_dense(),
                        "asm kernel disagreed on tag {tag:#04x} for {ctrl:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn vacant_backend_matches_scalar_reference() {
        let mut state = 0x1234_5678_9ABC_DEF0u64;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };

        for _ in 0..10_000 {
            let mut ctrl = [0u8; GROUP_WIDTH];
            for chunk in ctrl.chunks_mut(8) {
                chunk.copy_from_slice(&next().to_le_bytes()[..chunk.len()]);
            }
            assert_eq!(
                ctrl_match_vacant(&ctrl).to_dense(),
                ctrl_match_vacant_scalar(&ctrl).to_dense(),
                "backend {} disagreed for {ctrl:?}",
                backend_name()
            );
        }
    }

    #[test]
    fn vacant_separates_full_from_empty_and_deleted() {
        let mut ctrl = [0x00u8; GROUP_WIDTH]; // all FULL, tag 0
        ctrl[3] = 0xFF; // EMPTY
        ctrl[11] = 0x80; // DELETED
        ctrl[7] = 0x7F; // FULL with the highest tag -- must not count as vacant
        assert_eq!(
            ctrl_match_vacant(&ctrl).iter().collect::<Vec<_>>(),
            vec![3, 11]
        );
    }

    #[test]
    fn finds_every_single_slot() {
        for slot in 0..GROUP_WIDTH {
            let mut ctrl = [0xFFu8; GROUP_WIDTH];
            ctrl[slot] = 0x42;
            let mask = ctrl_match(&ctrl, 0x42);
            assert_eq!(mask.lowest_slot(), Some(slot));
            assert_eq!(mask.iter().collect::<Vec<_>>(), vec![slot]);
        }
    }

    #[test]
    fn reports_all_matches_in_order() {
        let mut ctrl = [0x00u8; GROUP_WIDTH];
        for slot in [0usize, 3, 7, 8, 15] {
            ctrl[slot] = 0x42;
        }
        let mask = ctrl_match(&ctrl, 0x42);
        assert_eq!(mask.iter().collect::<Vec<_>>(), vec![0, 3, 7, 8, 15]);
    }

    #[test]
    fn empty_group_yields_no_match() {
        let ctrl = [0x00u8; GROUP_WIDTH];
        let mask = ctrl_match(&ctrl, 0x42);
        assert!(mask.is_empty());
        assert_eq!(mask.lowest_slot(), None);
        assert_eq!(mask.iter().count(), 0);
    }

    #[test]
    fn full_group_yields_every_slot() {
        let ctrl = [0x42u8; GROUP_WIDTH];
        let mask = ctrl_match(&ctrl, 0x42);
        assert_eq!(mask.iter().count(), GROUP_WIDTH);
        assert_eq!(mask.to_dense(), u16::MAX);
    }
}
