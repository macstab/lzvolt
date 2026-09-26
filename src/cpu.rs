//! Runtime CPU feature detection.
//!
//! The codec dispatches on two things and not one. The instruction set says
//! what a body *may* execute -- the Xeon and EPYC decoders copy 256 bits at a
//! time, so calling one on a part without AVX2 is an illegal instruction rather
//! than a slow answer. The part line says which body is *worth* running, and
//! that is a separate question with a measured answer: the same source has come
//! back with opposite signs on two parts often enough that "x86" stopped being
//! one target here.
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
    /// Fast Short REP MOV. Ice Lake and later make a short `rep movsb` genuinely
    /// short; Zen has ERMSB but not that, and pays tens of cycles of start-up on
    /// the same copy. The decoder's last literal run is copied one way or the
    /// other depending on this, and it is worth 17% on an EPYC and -9% on a
    /// Xeon -- the same change, opposite signs, which is why it is asked rather
    /// than chosen.
    pub fsrm: bool,
    /// Which house built the part. The decoder is compiled once per vendor now,
    /// because the same code has measured opposite signs on the two -- the last
    /// literal run wants `rep movsb` on a Xeon and a copy ladder on an EPYC, and
    /// that is unlikely to be the only place. Anything that is neither gets the
    /// SSE2 kernel.
    pub intel: bool,
    pub amd: bool,
    /// The server line, which is not the same question as the vendor. A Core
    /// laptop part and a Sapphire Rapids Xeon share a house and little else --
    /// different cache sizes, different core mixes, and the string-copy answer
    /// that a Xeon wants is not one a client part has been measured on. Tuning
    /// is per line; anything unrecognised gets the plain SSSE3 kernel, which
    /// assumes nothing about either.
    pub xeon: bool,
    pub epyc: bool,
    pub avx2: bool,
    pub avx512f: bool,
    pub neon: bool,
    pub sve: bool,
    /// Arm's Neoverse V2, which is Google's Axion and Graviton4 and Grace.
    ///
    /// The AArch64 side had no part line at all until this: one body per split,
    /// shared between an M2 Max and a server core. That held while every
    /// measurement agreed, and it stopped holding when one did not -- the same
    /// change to the literal loop is worth +7.9% on the M2 and -1.4% here.
    ///
    /// Matched exactly, by MIDR part number, and nothing else. A Neoverse N2 is
    /// a different core with a different store width and has not been measured,
    /// so it gets the generic body, which assumes nothing. Same rule the x86
    /// side already follows for a brand string it does not recognise.
    pub neoverse_v2: bool,
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

/// Whether a brand string names a part line, ignoring case.
///
/// Pulled out of [`detect`] so it can be tested against real strings, which is
/// the only way this is testable at all — everything around it needs CPUID.
///
/// Case-insensitive because the vendors do not agree with themselves. A
/// Sapphire Rapids part writes `Intel(R) Xeon(R) Platinum 8481C`; an Emerald
/// Rapids part writes `INTEL(R) XEON(R) PLATINUM 8581C`. Matching `Xeon`
/// exactly classified the second as an unrecognised part and ran the generic
/// kernel on it — silently, because the generic kernel is correct, just not the
/// one tuned for that machine. It was found by a profile that reported no
/// samples in the symbol it was looking for.
///
/// `line` must already be upper case.
#[cfg(target_arch = "x86_64")]
fn brand_says(brand: &str, line: &str) -> bool {
    debug_assert_eq!(line, line.to_ascii_uppercase());
    brand
        .as_bytes()
        .windows(line.len())
        .any(|w| w.eq_ignore_ascii_case(line.as_bytes()))
}

/// Ask the machine once, at the first call to [`features`].
///
/// Everything here is a question with a defined answer -- CPUID leaves on
/// x86-64, MIDR_EL1 through sysfs on AArch64 -- and nothing is inferred from
/// anything else. A part whose brand string names no line we have measured
/// comes back with `xeon` and `epyc` both false, and dispatch then picks the
/// body that assumes nothing about either, which is the right answer for
/// hardware nobody has benchmarked.
fn detect() -> Features {
    let mut f = Features::default();

    #[cfg(target_arch = "x86_64")]
    {
        // SSE2 is part of the x86-64 baseline, so this is unconditionally true;
        // it is recorded explicitly so `INFO` output stays self-describing.
        f.sse2 = true;
        f.ssse3 = std::arch::is_x86_feature_detected!("ssse3");
        // No `is_x86_feature_detected!` name for this one; it is CPUID leaf 7,
        // sub-leaf 0, EDX bit 4. The leaf itself exists on everything that has
        // SSSE3, so no maximum-leaf check is needed here.
        // SAFETY: CPUID with a leaf every x86-64 part supports.
        f.fsrm = unsafe { std::arch::x86_64::__cpuid_count(7, 0).edx & (1 << 4) != 0 };
        // Leaf 0 returns the vendor string in EBX, EDX, ECX -- in that order,
        // which is why it reads as "Genu" "ineI" "ntel" rather than in register
        // order.
        // SAFETY: leaf 0 exists on every x86 part that has CPUID at all.
        let v = unsafe { std::arch::x86_64::__cpuid(0) };
        let vendor = [v.ebx, v.edx, v.ecx];
        f.intel = vendor == [0x756e_6547, 0x4965_6e69, 0x6c65_746e];
        f.amd = vendor == [0x6874_7541, 0x6974_6e65, 0x444d_4163];

        // The line name comes from the brand string, leaves 0x80000002..4,
        // because that is where the vendor writes what it calls the part. A
        // translator or a hypervisor may write something else entirely --
        // Rosetta says "VirtualApple" -- and then neither line matches and the
        // generic kernel runs, which is the right answer for an unknown part.
        let mut brand = [0u8; 48];
        // SAFETY: extended leaves; the maximum is checked first.
        let max_ext = unsafe { std::arch::x86_64::__cpuid(0x8000_0000).eax };
        if max_ext >= 0x8000_0004 {
            for (i, leaf) in [0x8000_0002u32, 0x8000_0003, 0x8000_0004].iter().enumerate() {
                // SAFETY: leaf is at or below the maximum reported above.
                let c = unsafe { std::arch::x86_64::__cpuid(*leaf) };
                for (j, r) in [c.eax, c.ebx, c.ecx, c.edx].iter().enumerate() {
                    brand[i * 16 + j * 4..i * 16 + j * 4 + 4].copy_from_slice(&r.to_le_bytes());
                }
            }
        }
        let brand = String::from_utf8_lossy(&brand);
        f.xeon = f.intel && brand_says(&brand, "XEON");
        f.epyc = f.amd && brand_says(&brand, "EPYC");
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
        f.neoverse_v2 = midr_says_neoverse_v2();
    }

    f
}

/// Whether core zero is a Neoverse V2, read from its MIDR.
///
/// AArch64 has no CPUID. The identification register is privileged, so Linux
/// exports it per core under sysfs; there is no such file on macOS, and an
/// Apple part therefore answers no and runs the generic body -- which is what
/// every measurement on it was taken with.
///
/// Core zero and not a survey of all of them. On a server the cores are alike;
/// on a phone they are not, and a phone is not a deployment target here. If that
/// ever changes, the answer to a heterogeneous machine is the generic body, and
/// reading one core that happens to be big would be the wrong way to get it.
#[cfg(target_arch = "aarch64")]
fn midr_says_neoverse_v2() -> bool {
    const PATH: &str = "/sys/devices/system/cpu/cpu0/regs/identification/midr_el1";
    let Ok(text) = std::fs::read_to_string(PATH) else {
        return false;
    };
    let Ok(midr) = u64::from_str_radix(text.trim().trim_start_matches("0x"), 16) else {
        return false;
    };
    midr_is_neoverse_v2(midr)
}

/// The bit fields of MIDR_EL1, split out so they can be tested without one.
///
/// Implementer in bits 31..24 and part number in 15..4. Arm is 0x41 and
/// Neoverse V2 is 0xd4f. Apple is 0x61, which is why an M-series part cannot
/// match even if the file somehow existed.
#[cfg(target_arch = "aarch64")]
fn midr_is_neoverse_v2(midr: u64) -> bool {
    let implementer = (midr >> 24) & 0xff;
    let part = (midr >> 4) & 0xfff;
    implementer == 0x41 && part == 0xd4f
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


    /// The two spellings of the same word, from two real parts.
    ///
    /// `8481C` is what a `c3` instance reports and `8581C` what a `c4` does.
    /// The second was classified as an unrecognised part for as long as the
    /// match was case-sensitive, so it never ran the kernel written for it.
    #[cfg(target_arch = "x86_64")]
    #[test]
    fn a_part_line_is_recognised_whatever_its_case() {
        for brand in [
            "Intel(R) Xeon(R) Platinum 8481C CPU @ 2.70GHz",
            "INTEL(R) XEON(R) PLATINUM 8581C CPU @ 2.30GHz",
        ] {
            assert!(super::brand_says(brand, "XEON"), "missed {brand}");
            assert!(!super::brand_says(brand, "EPYC"), "false EPYC on {brand}");
        }
        assert!(super::brand_says("AMD EPYC 9B14", "EPYC"));
        assert!(!super::brand_says("AMD EPYC 9B14", "XEON"));

        // A part nobody has measured, and a translator that writes its own
        // name: neither line matches and the generic kernel runs, which is the
        // right answer.
        for other in ["12th Gen Intel(R) Core(TM) i7-1260P", "VirtualApple @ 2.50GHz"] {
            assert!(!super::brand_says(other, "XEON"));
            assert!(!super::brand_says(other, "EPYC"));
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
            fsrm: false,
            intel: false,
            amd: false,
            xeon: false,
            epyc: false,
            avx2: true,
            avx512f: true,
            ..Features::default()
        };
        assert_eq!(f.best(), Isa::Avx2, "AVX-512 must stay opt-in");
    }
}
