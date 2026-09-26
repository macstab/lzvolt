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
//! Every kernel has a portable reference in `format`, reachable by building
//! without the `asm` feature, and the differential tests assert that the two
//! agree. The portable path is a supported configuration, not a
//! fallback of last resort: building with `--no-default-features` disables the
//! assembly entirely and must still pass the full test suite.

#![cfg_attr(docsrs, feature(doc_cfg))]

// The group probe that used to live here -- `ctrl_match`, `BitMask`, their
// scalar and intrinsic backends, and the group width they were built around --
// stayed behind in the store it belongs to. It shared no symbol, no constant
// and no file with the codec, which is what made this split a matter of moving
// files rather than untangling them.

/// Finding matches: the hash table walk and the assembled packers.
pub mod encode;

/// Reading a stream back: the dispatch across split, format and part line.
pub mod decode;

pub const fn asm_enabled() -> bool {
    cfg!(all(
        lzvolt_asm,
        any(target_arch = "aarch64", target_arch = "x86_64")
    ))
}

/// Human-readable name of the selected backend, for labelling a measurement.
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
mod abi {
    /// Every callee-saved register the kernels write must be saved on entry.
    ///
    /// This is checked in the source rather than by running anything, because
    /// running cannot check it. A kernel that clobbers a callee-saved register
    /// corrupts its *caller*, so whether the damage shows depends on whether
    /// that caller happened to be using the register -- the same build passes
    /// or fails depending on code that is not being tested. It cost three
    /// commits once: `stp x27, x28` was removed because x27 was dead, which
    /// dropped x28's save with it, and every output-comparing test stayed green
    /// because the kernel merely declined and the portable decoder answered.
    #[test]
    fn callee_saved_registers_are_all_saved() {
        for file in ["asm/aarch64/unpack.S", "asm/aarch64/pack.S"] {
            let src = std::fs::read_to_string(file).expect(file);
            // Only x19-x28 matter; x29 and x30 are the frame and link, and the
            // prologue would not assemble without them.
            for n in 19..=28u32 {
                let written = src.lines().any(|l| {
                    let l = l.trim();
                    !l.starts_with("//")
                        && !l.starts_with('*')
                        && !l.starts_with("st")
                        && !l.starts_with("ld")
                        && l.split_once(char::is_whitespace).is_some_and(|(_, rest)| {
                            let first = rest.trim_start().split(',').next().unwrap_or("").trim();
                            first == format!("x{n}") || first == format!("w{n}")
                        })
                });
                if !written {
                    continue;
                }
                let saved = src.lines().any(|l| {
                    let l = l.trim();
                    (l.starts_with("stp") || l.starts_with("str"))
                        && l.contains("[sp")
                        && (l.contains(&format!("x{n},")) || l.contains(&format!("x{n} ")))
                });
                assert!(saved, "{file} writes x{n} and never saves it");
            }
        }
    }
}
