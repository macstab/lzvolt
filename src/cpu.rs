//! Runtime CPU feature detection.
//!
//! [`ctrl_match`](crate::ctrl_match) does not use this: SSE2 and NEON are
//! architectural baselines, so its kernel is selected at compile time with no
//! branch. Dispatch exists for the *wide* kernels that are coming next -- the
//! expiry sweep and eviction sampling, which stream over millions of entries
//! and where 256- or 512-bit vectors genuinely pay.
//!
//! Two things worth knowing before reaching for AVX-512:
//!
//! - It is fused off on Alder Lake and later consumer parts, and on older Xeon
//!   it triggers frequency throttling that can make surrounding scalar code
//!   slower. It has to earn its place per workload, measured on the target part.
//! - SVE is not a safe assumption on ARM. NEON is the baseline; SVE shows up on
//!   Graviton3 and later and on a handful of server parts, so it is strictly a
//!   dispatch target.

use std::sync::OnceLock;

/// The widest kernel family usable on this machine.
///
/// Ordering is by vector width, not by preference -- see [`Features::best`],
/// which applies the policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Isa {
    /// Portable reference path. Always correct, always available.
    Scalar,
    /// x86-64 baseline, 128-bit.
    Sse2,
    /// 256-bit x86.
    Avx2,
    /// 512-bit x86. Gated behind measurement, see the module docs.
    Avx512,
    /// AArch64 baseline, 128-bit.
    Neon,
    /// Scalable vectors on AArch64. Graviton3+ and similar.
    Sve,
}

impl Isa {
    /// Name reported by `INFO`, so a benchmark number can always be traced to
    /// the code path that produced it.
    pub const fn as_str(self) -> &'static str {
        match self {
            Isa::Scalar => "scalar",
            Isa::Sse2 => "sse2",
            Isa::Avx2 => "avx2",
            Isa::Avx512 => "avx512",
            Isa::Neon => "neon",
            Isa::Sve => "sve",
        }
    }
}

/// Detected capabilities of the current machine.
#[derive(Debug, Clone, Copy, Default)]
pub struct Features {
    pub sse2: bool,
    /// Byte shuffles. The decoder builds a near match's whole pattern with one
    /// `pshufb` instead of growing it a byte at a time, so this is worth
    /// dispatching on even though every server part since 2007 has it.
    pub ssse3: bool,
    pub avx2: bool,
    pub avx512f: bool,
    pub neon: bool,
    pub sve: bool,
}

impl Features {
    /// The ISA the wide kernels should dispatch to.
    ///
    /// AVX-512 is intentionally *not* selected automatically. Enabling it is a
    /// per-deployment decision backed by a measurement on the actual part, for
    /// the throttling reasons in the module docs, so it is opt-in through
    /// configuration rather than implied by the CPUID bit being set.
    pub fn best(self) -> Isa {
        if self.sve {
            Isa::Sve
        } else if self.neon {
            Isa::Neon
        } else if self.avx2 {
            Isa::Avx2
        } else if self.sse2 {
            Isa::Sse2
        } else {
            Isa::Scalar
        }
    }
}

/// Detect once, then hand out the cached answer.
pub fn features() -> Features {
    static CACHE: OnceLock<Features> = OnceLock::new();
    *CACHE.get_or_init(detect)
}

/// Convenience wrapper over [`features`].
pub fn best_isa() -> Isa {
    features().best()
}

fn detect() -> Features {
    let mut f = Features::default();

    #[cfg(target_arch = "x86_64")]
    {
        // SSE2 is part of the x86-64 baseline, so this is unconditionally true;
        // it is recorded explicitly so `INFO` output stays self-describing.
        f.sse2 = true;
        f.ssse3 = std::arch::is_x86_feature_detected!("ssse3");
        f.avx2 = std::arch::is_x86_feature_detected!("avx2");
        f.avx512f = std::arch::is_x86_feature_detected!("avx512f");
    }

    #[cfg(target_arch = "aarch64")]
    {
        // NEON is mandatory in AArch64.
        f.neon = true;
        // SVE detection needs HWCAP2 on Linux and is absent on Apple silicon
        // entirely. Left false until the first SVE kernel lands, so we never
        // claim a capability no kernel actually uses.
        f.sve = false;
    }

    f
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detection_is_consistent_with_target() {
        let f = features();
        if cfg!(target_arch = "x86_64") {
            assert!(f.sse2, "SSE2 is part of the x86-64 baseline");
        }
        if cfg!(target_arch = "aarch64") {
            assert!(f.neon, "NEON is mandatory in AArch64");
        }
    }

    #[test]
    fn detection_is_cached() {
        let a = features();
        let b = features();
        assert_eq!(a.best(), b.best());
    }

    #[test]
    fn avx512_is_never_selected_implicitly() {
        let f = Features {
            sse2: true,
            ssse3: true,
            avx2: true,
            avx512f: true,
            ..Features::default()
        };
        assert_eq!(f.best(), Isa::Avx2, "AVX-512 must stay opt-in");
    }
}
