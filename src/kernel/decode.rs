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

#[cfg(all(keva_asm, target_arch = "aarch64"))]
extern "C" {
    fn keva_unpack(
        src: *const u8,
        src_len: usize,
        dst: *mut u8,
        dst_cap: usize,
        declared: usize,
    ) -> u32;
}

/// Whether this build has an assembly decoder.
pub const fn asm_available() -> bool {
    cfg!(all(keva_asm, target_arch = "aarch64"))
}

/// Decode `body` — the packed stream with its length header already removed —
/// into `out`, which is replaced.
///
/// Returns `false` to mean "use the portable decoder": either there is no
/// kernel on this target, or the kernel declined. It never means the value is
/// definitely corrupt, so the caller must fall back rather than report an
/// error.
pub fn unpack_asm(body: &[u8], out: &mut Vec<u8>, declared: usize) -> bool {
    #[cfg(not(all(keva_asm, target_arch = "aarch64")))]
    {
        let _ = (body, out, declared);
        false
    }

    #[cfg(all(keva_asm, target_arch = "aarch64"))]
    {
        // Zero is how the kernel says it declined, so it cannot also be how it
        // reports success. An empty value is the portable decoder's business.
        if declared == 0 {
            return false;
        }

        out.clear();
        out.reserve(declared + UNPACK_SLACK);
        let cap = out.capacity();

        // SAFETY: `cap` is the real capacity, so the whole range handed to the
        // kernel is allocated. The kernel writes below `declared + 64` or
        // returns zero, and it is handed `cap` so it can verify that itself.
        // Nothing reads the slack: the length only moves out to `declared`, and
        // every byte below it was written by this call.
        let produced = unsafe {
            keva_unpack(
                body.as_ptr(),
                body.len(),
                out.as_mut_ptr(),
                cap,
                declared,
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
