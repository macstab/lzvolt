//! A byte-oriented LZ77 packer for entry values.
//!
//! Written rather than pulled in, for the same reason the protocol crate has no
//! dependencies: [`unpack`] reads bytes that came from a socket or a snapshot
//! file, so every length in them is attacker-controlled. Code on that path
//! should be short enough to audit in one sitting.
//!
//! # Format
//!
//! ```text
//! [original length: varint][block][block]...
//!
//! block := [token: u8][extended literal length][literals][offset: u16][extended match length]
//!          token high nibble = literal count, 15 means "read more"
//!          token low nibble  = match length - 4, 15 means "read more"
//! ```
//!
//! The final block carries literals and no match, which is what terminates the
//! stream — decoding stops when the declared length has been produced.
//!
//! # What it is tuned for
//!
//! Values, not archives. A four-byte minimum match and a 16-bit offset keep the
//! inner loop to a hash lookup and a comparison, which is where the throughput
//! comes from; the ratio is deliberately traded away for it. On a store where
//! reads outnumber writes ten to one, the asymmetry is the point: unpacking is
//! a `memcpy` loop and costs a fraction of what packing does.
//!
//! # Verification
//!
//! [`unpack`] copies with raw pointers, so the checks in front of those copies
//! are the only thing between a malformed value and memory unsafety. Three
//! things stand behind that: every truncation and every single-bit flip of a
//! packed stream, thousands of multi-byte corruptions and streams that were
//! never packed at all, and all of it run under Miri, which reports
//! out-of-bounds access and uninitialised reads that a passing test would not.
//!
//! `cargo +nightly miri test -p keva-core --lib store::pack`
//!
//! That belongs in CI. A change here that keeps the tests green but drops a
//! bounds check is exactly the failure this module is shaped to prevent.
//!
//! # What it does not do
//!
//! Entropy coding. That is where the remaining ratio lives, and it would cost
//! several times the CPU on both sides. If a namespace needs archival density
//! it wants a different tool, not a slower version of this one.

use crate::store::entry::{get_varint, put_varint};

/// Shortest run worth encoding as a reference rather than as literals.
///
/// Below four bytes a match costs more than it saves: the token plus a two-byte
/// offset is three bytes already.
const MIN_MATCH: usize = 4;

/// Longest backward reference. Sixteen bits is the whole reason the inner loop
/// stays cheap, and values are small enough that a longer window would find
/// almost nothing.
const MAX_OFFSET: usize = 65_535;

/// Positions indexed by a hash of the four bytes starting there.
///
/// A single slot per hash, so a collision simply loses a match rather than
/// costing a search. That is the trade that keeps packing near memory speed.
///
/// Eleven bits, not twelve, and the reason is the cache rather than the hash.
/// The table shares L1 with the data it indexes, so every slot it gains is a
/// line the input loses. Measured on a 64 KiB value, where the two are actually
/// in contention:
///
/// ```text
///   bits   table     GiB/s   ratio
///     10    4 KiB     0.78    1.86x
///     11    8 KiB     0.72    1.91x
///     12   16 KiB     0.60    1.96x
///     13   32 KiB     0.55    1.94x
/// ```
///
/// Halving the table buys 21% and costs 2.5% of the ratio; halving it again
/// buys 30% and costs 5%. Eleven is where that trade stops being obviously
/// worth it. On a 4 KiB value, where nothing is in contention, eleven measures
/// 1% ahead of twelve at an identical ratio -- so this is not a compromise for
/// small values, it is simply better there too.
const HASH_BITS: usize = 12;

/// Table bits for a value large enough that the table competes with it for L1.
///
/// The full table is always allocated; this only narrows how much of it a large
/// value touches, and touched lines are what the cache actually pays for.
const HASH_BITS_LARGE: usize = 11;

/// Where the table stops being free and starts being a rival for cache.
///
/// Below this a value and the whole table sit in L1 together, and the wider
/// table is simply better: fewer collisions, so fewer verify loads that go to a
/// random address only to fail. Incompressible data is where that shows up
/// hardest, since every collision there is pure waste -- 4 KiB of noise packs at
/// 4.47 GiB/s with twelve bits and 3.74 with eleven.
///
/// Above it the relationship inverts, because the table is displacing the data:
/// a 64 KiB value packs at 0.61 GiB/s with twelve bits and 0.87 with eleven.
const NARROW_TABLE_ABOVE: usize = 8192;
const HASH_SIZE: usize = 1 << HASH_BITS;

/// A slot that has never been written.
///
/// Zero would be the obvious choice and it costs a branch: the search would
/// have to ask "is this slot empty" before asking "is this candidate usable",
/// and the first question is only ever asked to avoid mis-answering the second.
///
/// Any value far above the largest reachable position folds the two into one.
/// The usability test computes `at - stored` and rejects anything at or past
/// the window; a sentinel this large makes that subtraction wrap, so an empty
/// slot fails the test already there. No position can collide with it, since a
/// value would have to be two gigabytes long to reach it.
const EMPTY: u32 = 0x8000_0000;

/// The match-finding table, held across calls.
///
/// Clearing this was the dominant cost for small values: sixteen kilobytes of
/// initialisation against a 512-byte value is thirty-two times more setup than
/// work, and measured at roughly 88% of the time such a call took.
///
/// It is never cleared. An entry left from a previous value is not a
/// correctness problem, because a candidate is only used after being checked
/// against the current position and then verified byte for byte — a stale one
/// yields a short match that is discarded like any other. The cost is a
/// slightly worse ratio in the first few bytes of a value, which is a good
/// trade for removing the setup entirely.
#[derive(Debug)]
pub struct Packer {
    /// Position plus one, with [`EMPTY`] for a slot never written.
    table: Box<[u32; HASH_SIZE]>,
}

impl Default for Packer {
    fn default() -> Self {
        Packer {
            table: Box::new([EMPTY; HASH_SIZE]),
        }
    }
}

impl Packer {
    pub fn new() -> Packer {
        Packer::default()
    }

    /// Pack `input` into `out`. See [`pack`] for the contract.
    pub fn pack(&mut self, input: &[u8], out: &mut Vec<u8>) -> bool {
        pack_with(input, out, &mut self.table)
    }
}

/// Why a packed value could not be read back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackError {
    /// The stream ended in the middle of a field.
    Truncated,
    /// A backward reference pointed before the start of the output.
    BadOffset,
    /// The stream declared a length it then did not produce, or exceeded it.
    LengthMismatch,
    /// The declared length was larger than any value may be.
    TooLarge,
}

impl std::fmt::Display for PackError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            PackError::Truncated => "packed value ends mid-field",
            PackError::BadOffset => "backward reference points before the output",
            PackError::LengthMismatch => "packed value does not produce its declared length",
            PackError::TooLarge => "packed value declares an implausible length",
        })
    }
}

impl std::error::Error for PackError {}

/// Largest value this will unpack, as a guard against a declared length that
/// would otherwise reserve gigabytes before failing.
const MAX_UNPACKED: usize = 512 * 1024 * 1024;

/// Hash the four bytes at the start of `bytes`.
///
/// Read as one word rather than four indexed bytes: the indexed form is four
/// bounds checks and four loads that the compiler is not always willing to
/// merge, and this runs once per position in the input.
#[inline]
fn hash4(bytes: &[u8], shift: u32) -> usize {
    let word = u32::from_le_bytes(bytes[..4].try_into().unwrap());
    (word.wrapping_mul(0x9E37_79B1) >> shift) as usize
}

/// How far to shift the hash for a value of this length. See
/// [`NARROW_TABLE_ABOVE`].
#[inline]
fn hash_shift(len: usize) -> u32 {
    if len > NARROW_TABLE_ABOVE {
        (32 - HASH_BITS_LARGE) as u32
    } else {
        (32 - HASH_BITS) as u32
    }
}

/// The four bytes at `at`, as one word.
///
/// `at + 4` is within the input at every call site: the main loop runs while
/// `at + MIN_MATCH <= input.len()`, and a candidate is always behind `at`.
#[inline]
fn word4(input: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(input[at..at + 4].try_into().unwrap())
}

/// How fast the search gives up. After `1 << SKIP_TRIGGER` consecutive misses
/// the cursor starts advancing by more than one byte, and keeps accelerating.
///
/// Without it, data that does not compress is walked one byte at a time to no
/// purpose — and even data that does compress spends most positions between
/// matches. The cost is a slightly worse ratio, since a skipped position is a
/// match never looked for; LZ4 uses the same trigger for the same reason.
const SKIP_TRIGGER: usize = 6;

/// Write a length that did not fit in a nibble: 255s until a smaller byte.
fn put_extended(mut remaining: usize, out: &mut Vec<u8>) {
    while remaining >= 255 {
        out.push(255);
        remaining -= 255;
    }
    out.push(remaining as u8);
}

/// Read the counterpart of [`put_extended`], bounded so a run of 255s cannot
/// spin forever on a corrupt stream.
fn get_extended(input: &[u8], at: &mut usize) -> Result<usize, PackError> {
    let mut total = 0usize;
    loop {
        let byte = *input.get(*at).ok_or(PackError::Truncated)?;
        *at += 1;
        total = total
            .checked_add(byte as usize)
            .ok_or(PackError::TooLarge)?;
        if byte != 255 {
            return Ok(total);
        }
        if total > MAX_UNPACKED {
            return Err(PackError::TooLarge);
        }
    }
}

/// Pack `input` into `out`, replacing whatever it held.
///
/// Returns `false` when the result would not be smaller, which is the common
/// case for short values and for anything already compressed. The caller stores
/// the original in that case, so a namespace holding JPEGs pays the packing
/// attempt and nothing else.
pub fn pack(input: &[u8], out: &mut Vec<u8>) -> bool {
    let mut table = Box::new([EMPTY; HASH_SIZE]);
    pack_with(input, out, &mut table)
}

/// The body, with the match table supplied so it can outlive one call.
fn pack_with(input: &[u8], out: &mut Vec<u8>, table: &mut [u32; HASH_SIZE]) -> bool {
    out.clear();
    if input.len() < MIN_MATCH {
        return false;
    }

    put_varint_into(input.len() as u64, out);

    out.reserve(input.len() / 4);

    let mut at = 0usize;
    let mut literal_start = 0usize;
    // Starts at the trigger, so the first step is one byte. Starting at one
    // would make the shift zero and the cursor would sit on the same position
    // for the first sixty-four attempts.
    let mut misses = 1usize << SKIP_TRIGGER;
    let shift = hash_shift(input.len());

    while at + MIN_MATCH <= input.len() {
        let slot = hash4(&input[at..], shift);
        let stored = table[slot];
        table[slot] = at as u32 + 1;

        // A slot holds a position from this value or a previous one, and only
        // positions behind the cursor are usable. An untouched slot holds
        // [`EMPTY`], which is far enough ahead of any real position that the
        // same test rejects it — see the constant.
        //
        // The four bytes are compared inline before anything else happens. On
        // data that varies, almost every candidate fails here — and the
        // previous form paid two bounds-checked slice constructions and a call
        // to find that out. Since the hash is of exactly these four bytes, a
        // candidate that disagrees on them cannot match at all, so this rejects
        // without touching the extension loop.
        let candidate = stored as usize - 1;
        let matched = if candidate < at
            && at - candidate <= MAX_OFFSET
            && word4(input, candidate) == word4(input, at)
        {
            MIN_MATCH + common_prefix(&input[candidate + MIN_MATCH..], &input[at + MIN_MATCH..])
        } else {
            0
        };

        if matched < MIN_MATCH {
            at += misses >> SKIP_TRIGGER;
            misses += 1;
            continue;
        }
        misses = 1 << SKIP_TRIGGER;

        // Walk the match backwards into the literals that were about to be
        // emitted. The bytes are already known to be equal there; they were
        // simply never looked at, because the search only ever moves forward.
        //
        // This is worth more than the ratio it adds. Every byte moved out of a
        // literal run and into a match is a byte the decoder copies in a block
        // instead of individually, and — since it shortens the run rather than
        // splitting it — it produces fewer, longer blocks. Decoding is bounded
        // by per-block work, so fewer blocks is the lever.
        let mut back = 0usize;
        while candidate > back
            && at - back > literal_start
            && input[candidate - back - 1] == input[at - back - 1]
        {
            back += 1;
        }

        emit_block(
            &input[literal_start..at - back],
            at - candidate,
            matched + back,
            out,
        );

        at += matched;
        literal_start = at;
    }

    // Everything left over is literals, with no match to follow.
    emit_literals_only(&input[literal_start..], out);

    out.len() < input.len()
}

fn put_varint_into(value: u64, out: &mut Vec<u8>) {
    let mut buf = [0u8; 10];
    let used = put_varint(&mut buf, value);
    out.extend_from_slice(&buf[..used]);
}

/// Length of the shared prefix, eight bytes at a time.
///
/// A byte-wise loop costs a load, a compare and a branch per byte; a 64-bit
/// load turns eight of those into one XOR whose trailing zero count gives the
/// exact position of the first difference. The tail below eight bytes falls
/// back to the simple loop.
fn common_prefix(a: &[u8], b: &[u8]) -> usize {
    let limit = a.len().min(b.len());
    let mut i = 0;

    while i + 8 <= limit {
        let left = u64::from_le_bytes(a[i..i + 8].try_into().unwrap());
        let right = u64::from_le_bytes(b[i..i + 8].try_into().unwrap());
        let diff = left ^ right;
        if diff != 0 {
            // Little-endian, so the first differing byte is the lowest set bit.
            return i + (diff.trailing_zeros() / 8) as usize;
        }
        i += 8;
    }

    while i < limit && a[i] == b[i] {
        i += 1;
    }
    i
}

fn emit_block(literals: &[u8], offset: usize, matched: usize, out: &mut Vec<u8>) {
    let match_extra = matched - MIN_MATCH;
    let literal_nibble = literals.len().min(15);
    let match_nibble = match_extra.min(15);
    out.push(((literal_nibble as u8) << 4) | match_nibble as u8);

    if literals.len() >= 15 {
        put_extended(literals.len() - 15, out);
    }
    out.extend_from_slice(literals);

    out.extend_from_slice(&(offset as u16).to_le_bytes());
    if match_extra >= 15 {
        put_extended(match_extra - 15, out);
    }
}

fn emit_literals_only(literals: &[u8], out: &mut Vec<u8>) {
    let literal_nibble = literals.len().min(15);
    // A zero match nibble with no offset following is what marks the tail; the
    // decoder knows to stop because the declared length has been reached.
    out.push((literal_nibble as u8) << 4);
    if literals.len() >= 15 {
        put_extended(literals.len() - 15, out);
    }
    out.extend_from_slice(literals);
}

/// Lay down a match whose source overlaps the output it is producing.
///
/// The naive form copies `offset` bytes at a time, so a run of one repeated
/// byte costs one call per byte. Instead the chunk doubles each round while the
/// read always starts at `src`: after writing `n` bytes, the pattern at `src`
/// is `offset + n` bytes long, so the next round may take twice what the last
/// one did. A hundred-byte run at offset one becomes seven copies rather than a
/// hundred.
///
/// This is the case LZ4 handles with jump tables and ClickHouse with `pshufb`.
/// Doubling needs neither, and costs the same on both architectures.
///
/// # Safety
///
/// `src` must equal `dst.sub(offset)` with `offset >= 1`, the pattern at `src`
/// must be initialised for `offset` bytes, and `dst` must be writable for
/// `len` bytes.
#[inline]
unsafe fn overlapping_copy(dst: *mut u8, src: *const u8, offset: usize, len: usize) {
    let mut written = 0usize;
    let mut chunk = offset;
    while written < len {
        let take = chunk.min(len - written);
        std::ptr::copy_nonoverlapping(src, dst.add(written), take);
        written += take;
        chunk *= 2;
    }
}

/// Unpack into `out`, replacing whatever it held.
///
/// Every length and offset in `input` is treated as hostile: the declared size
/// is capped before anything is reserved, each field is bounds-checked, and a
/// backward reference is verified against what has actually been produced.
/// Bytes a block copy may write past the logical end of the output.
///
/// The whole point of copying in fixed blocks is not having to trim the last
/// one, so the buffer is over-allocated by a block and the length is set to the
/// declared size at the end. Nothing ever reads the slack.
const OVERRUN: usize = 32;

/// Input bytes the fast path must be able to read past the token.
///
/// Computed once rather than per block: the guard runs on every block, and a
/// `max` there is arithmetic the loop does not need.
const FAST_INPUT_SLACK: usize = if FAST_LITERAL > OVERRUN {
    FAST_LITERAL
} else {
    OVERRUN
};

/// Bytes the fast path copies for a literal run.
///
/// The short form carries at most fourteen, so sixteen covers it. Copying a
/// full [`OVERRUN`] there would move twice the bytes for no gain, and on data
/// that compresses poorly literals are most of the output.
const FAST_LITERAL: usize = 16;

/// Copy `len` bytes from `src` to `dst` in fixed blocks, overrunning freely.
///
/// # Safety
///
/// `src` must be readable for `len.next_multiple_of(OVERRUN)` bytes and `dst`
/// writable for the same. The caller establishes both before calling; that is
/// the entire reason this is not doing it itself.
#[inline]
unsafe fn wildcopy(mut dst: *mut u8, mut src: *const u8, len: usize) {
    let mut copied = 0usize;
    loop {
        std::ptr::copy_nonoverlapping(src, dst, OVERRUN);
        copied += OVERRUN;
        if copied >= len {
            return;
        }
        src = src.add(OVERRUN);
        dst = dst.add(OVERRUN);
    }
}

/// Unpack into `out`, replacing whatever it held.
///
/// Every length and offset in `input` is treated as hostile: the declared size
/// is capped before anything is reserved, each field is bounds-checked, and a
/// backward reference is verified against what has actually been produced.
///
/// # How the unchecked copies are made safe
///
/// The copies below use raw pointers, and each one is preceded by the check
/// that makes it sound rather than relying on a later one to catch it:
///
/// - the output is reserved for `declared + OVERRUN`, so a block copy may
///   overrun the logical end by up to one block without leaving the allocation;
/// - a literal run is copied only after `at + literal_len` has been shown to be
///   within `input`, and blockwise only when a further `OVERRUN` bytes are also
///   within it, falling back to an exact copy near the end of the stream;
/// - a match is copied only after its offset has been shown to be no larger
///   than what has already been produced, so it never reads uninitialised
///   memory, and only after `produced + match_len <= declared`;
/// - the length is set once, at the end, to `declared` — never to whatever the
///   overrun happened to reach.
///
/// The corruption test walks every truncation and every single-bit flip of a
/// packed stream. With the bounds moved out of the copies, that test is what
/// stands between a malformed value and memory unsafety, so it is not
/// optional.
pub fn unpack(input: &[u8], out: &mut Vec<u8>) -> Result<(), PackError> {
    out.clear();

    let (declared, header) = get_varint(input).ok_or(PackError::Truncated)?;
    let declared = usize::try_from(declared).map_err(|_| PackError::TooLarge)?;
    if declared > MAX_UNPACKED {
        return Err(PackError::TooLarge);
    }
    out.reserve(declared + OVERRUN);

    let capacity = out.capacity();
    let base = out.as_mut_ptr();
    let mut produced = 0usize;
    let mut at = header;

    while produced < declared {
        let token = *input.get(at).ok_or(PackError::Truncated)?;

        // The common block: neither length escaped its nibble, and there is
        // room to read and write whole copy units without checking either
        // again. Everything the slow path computes step by step is bounded
        // here by construction — a literal run is at most 14 bytes and a match
        // at most 18 — so the two copies run at fixed sizes and the arithmetic
        // between them disappears.
        //
        // LZ4 owes most of its decode speed to exactly this shape. Without it
        // a block costs a dozen branches; with it, four.
        let short_literal = (token >> 4) as usize;
        let short_match = (token & 0x0F) as usize;
        if short_literal != 15
            && short_match != 15
            && at + 1 + short_literal + 2 + FAST_INPUT_SLACK <= input.len()
            && produced + 64 <= capacity
        {
            let literal_at = at + 1;
            let offset_at = literal_at + short_literal;
            // Left bounds-checked deliberately. Replacing this with two
            // unchecked byte reads measured 4.5% *slower*: the checked slice
            // becomes one 16-bit load, the unchecked pair does not.
            let offset =
                u16::from_le_bytes(input[offset_at..offset_at + 2].try_into().unwrap()) as usize;
            let match_len = short_match + MIN_MATCH;
            let after_literals = produced + short_literal;

            // Validated before either copy, because the match may reach into
            // the literals this block is about to write.
            if offset == 0 || offset > after_literals {
                return Err(PackError::BadOffset);
            }
            if after_literals + match_len > declared {
                return Err(PackError::LengthMismatch);
            }

            unsafe {
                std::ptr::copy_nonoverlapping(
                    input.as_ptr().add(literal_at),
                    base.add(produced),
                    FAST_LITERAL,
                );
                let src = base.add(after_literals - offset);
                let dst = base.add(after_literals);
                if offset >= OVERRUN {
                    std::ptr::copy_nonoverlapping(src, dst, OVERRUN);
                } else {
                    overlapping_copy(dst, src, offset, match_len);
                }
            }

            produced = after_literals + match_len;
            at = offset_at + 2;
            continue;
        }

        at += 1;

        let mut literal_len = (token >> 4) as usize;
        if literal_len == 15 {
            literal_len += get_extended(input, &mut at)?;
        }

        let end = at.checked_add(literal_len).ok_or(PackError::TooLarge)?;
        if end > input.len() {
            return Err(PackError::Truncated);
        }
        if produced + literal_len > declared {
            return Err(PackError::LengthMismatch);
        }

        // Blockwise only while a whole block is readable from the input and
        // writable into the reservation; the tail of the stream takes the exact
        // path, because overrunning the input would read memory that is not
        // ours.
        if literal_len > 0 {
            let blocked = literal_len.next_multiple_of(OVERRUN);
            unsafe {
                if end + OVERRUN <= input.len() && produced + blocked <= capacity {
                    wildcopy(base.add(produced), input.as_ptr().add(at), literal_len);
                } else {
                    std::ptr::copy_nonoverlapping(
                        input.as_ptr().add(at),
                        base.add(produced),
                        literal_len,
                    );
                }
            }
            produced += literal_len;
        }
        at = end;

        if produced == declared {
            break;
        }

        let offset_bytes = input.get(at..at + 2).ok_or(PackError::Truncated)?;
        let offset = u16::from_le_bytes([offset_bytes[0], offset_bytes[1]]) as usize;
        at += 2;

        let mut match_len = (token & 0x0F) as usize;
        if match_len == 15 {
            match_len += get_extended(input, &mut at)?;
        }
        let match_len = match_len + MIN_MATCH;

        if offset == 0 || offset > produced {
            return Err(PackError::BadOffset);
        }
        if produced + match_len > declared {
            return Err(PackError::LengthMismatch);
        }

        // A match may overlap the output it is still producing — that is how a
        // repeated pattern is encoded. Blockwise copying is only sound when the
        // source is at least a block behind the destination; closer than that,
        // the copy would read bytes it has not written yet, so the pattern is
        // laid down offset by offset instead.
        unsafe {
            let src = base.add(produced - offset);
            let dst = base.add(produced);
            if offset >= OVERRUN && produced + match_len.next_multiple_of(OVERRUN) <= capacity {
                wildcopy(dst, src, match_len);
            } else {
                overlapping_copy(dst, src, offset, match_len);
            }
        }
        produced += match_len;
    }

    if produced != declared {
        return Err(PackError::LengthMismatch);
    }
    // Set once, to the declared size: the overrun above may have written past
    // it, and those bytes are not part of the value.
    unsafe { out.set_len(declared) };
    Ok(())
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

    fn roundtrip(input: &[u8]) {
        let mut packed = Vec::new();
        let mut unpacked = Vec::new();
        if pack(input, &mut packed) {
            unpack(&packed, &mut unpacked).expect("must unpack what it packed");
            assert_eq!(unpacked, input, "round trip changed the bytes");
        }
    }

    #[test]
    fn round_trips_repetitive_data() {
        let input = vec![b'x'; 4096];
        let mut packed = Vec::new();
        assert!(pack(&input, &mut packed));
        assert!(
            packed.len() < input.len() / 20,
            "4096 identical bytes packed to {} bytes",
            packed.len()
        );

        let mut out = Vec::new();
        unpack(&packed, &mut out).unwrap();
        assert_eq!(out, input);
    }

    #[test]
    fn round_trips_structured_data() {
        let mut input = Vec::new();
        for i in 0..500 {
            input.extend_from_slice(format!("{{\"user\":{i},\"active\":true}}").as_bytes());
        }
        let mut packed = Vec::new();
        assert!(pack(&input, &mut packed), "JSON-ish data must compress");

        let mut out = Vec::new();
        unpack(&packed, &mut out).unwrap();
        assert_eq!(out, input);
    }

    /// The property that keeps a namespace of JPEGs from paying twice: if
    /// packing did not help, the caller is told so and stores the original.
    #[test]
    fn refuses_when_it_would_not_help() {
        let mut incompressible = Vec::new();
        let mut state = 0x2545_F491_4F6C_DD1Du64;
        for _ in 0..4096 {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            incompressible.push(state as u8);
        }
        let mut packed = Vec::new();
        assert!(
            !pack(&incompressible, &mut packed),
            "random bytes must not claim to compress"
        );
    }

    #[test]
    fn handles_the_edges() {
        for input in [
            b"".as_slice(),
            b"a",
            b"ab",
            b"abc",
            b"abcd",
            b"abcde",
            &[0u8; 15],
            &[0u8; 16],
            &[0u8; 17],
            &[0xFFu8; 300],
        ] {
            roundtrip(input);
        }
    }

    /// A run longer than its own offset: the match overlaps what it is still
    /// producing, which is how `aaaa...` is encoded and where a bulk copy
    /// would read uninitialised bytes.
    #[test]
    fn overlapping_matches_round_trip() {
        let mut input = b"ab".to_vec();
        input.extend(std::iter::repeat(b'c').take(1000));
        input.extend_from_slice(b"ab");
        roundtrip(&input);

        let alternating: Vec<u8> = (0..2000)
            .map(|i| if i % 2 == 0 { b'a' } else { b'b' })
            .collect();
        roundtrip(&alternating);
    }

    /// Every byte of a packed value is attacker-controlled once it has been
    /// through a snapshot file. None of these may panic.
    #[test]
    fn corrupt_input_is_refused_rather_than_trusted() {
        let input = vec![b'q'; 2000];
        let mut packed = Vec::new();
        assert!(pack(&input, &mut packed));

        let mut out = Vec::new();

        // Truncation at every length.
        for cut in 0..packed.len() {
            let _ = unpack(&packed[..cut], &mut out);
        }

        // Every single-byte corruption.
        for i in 0..packed.len() {
            for bit in 0..8 {
                let mut broken = packed.clone();
                broken[i] ^= 1 << bit;
                let _ = unpack(&broken, &mut out);
            }
        }

        // A declared length nobody could hold.
        let mut absurd = Vec::new();
        put_varint_into(u64::MAX, &mut absurd);
        absurd.push(0);
        assert!(matches!(
            unpack(&absurd, &mut out),
            Err(PackError::TooLarge | PackError::Truncated)
        ));
    }

    /// The copies in `unpack` are unchecked, so the checks that precede them
    /// are the only thing between a malformed value and memory unsafety. Single
    /// bit flips are covered above; this adds the shapes a corrupt file or a
    /// hostile sender actually produces — several bytes wrong at once, a
    /// plausible header over a garbage body, and streams that were never packed
    /// at all.
    ///
    /// Nothing here asserts a particular error. The property under test is that
    /// none of it reads or writes out of bounds, which shows up as a crash or,
    /// under Miri, as undefined behaviour.
    #[test]
    fn hostile_input_stays_within_bounds() {
        let mut state = 0x9E37_79B9_7F4A_7C15u64;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };

        let source = records(3000);
        let mut packed = Vec::new();
        assert!(pack(&source, &mut packed));
        let mut out = Vec::new();

        // Several bytes corrupted at once, which single-bit coverage misses:
        // a length field can be made large and its guard byte made consistent
        // by the same edit.
        for _ in 0..4000 {
            let mut broken = packed.clone();
            let hits = 1 + (next() % 6) as usize;
            for _ in 0..hits {
                let at = (next() as usize) % broken.len();
                broken[at] = next() as u8;
            }
            let _ = unpack(&broken, &mut out);
        }

        // A believable declared length in front of noise.
        for len in [0u64, 1, 64, 4096, 1 << 20] {
            for _ in 0..200 {
                let mut stream = Vec::new();
                put_varint_into(len, &mut stream);
                for _ in 0..(next() % 200) {
                    stream.push(next() as u8);
                }
                let _ = unpack(&stream, &mut out);
            }
        }

        // Streams that were never packed.
        for _ in 0..2000 {
            let n = (next() % 300) as usize;
            let stream: Vec<u8> = (0..n).map(|_| next() as u8).collect();
            let _ = unpack(&stream, &mut out);
        }
    }

    /// A match whose offset is smaller than one copy block takes the overlapping
    /// path, and one that is larger takes the blockwise path. Both have to
    /// produce the same bytes, and the boundary between them is where a
    /// blockwise copy would read what it has not yet written.
    #[test]
    fn both_match_copy_paths_agree() {
        for offset in [1usize, 2, 3, 7, 8, 15, 16, 31, 32, 33, 64, 200] {
            let mut input = Vec::new();
            let pattern: Vec<u8> = (0..offset).map(|i| b'a' + (i % 26) as u8).collect();
            while input.len() < 4000 {
                input.extend_from_slice(&pattern);
            }
            input.truncate(4000);
            roundtrip(&input);
        }
    }

    /// The assembly packer must produce exactly what the Rust one produces.
    ///
    /// Not merely something that unpacks to the same bytes — the identical
    /// stream. A different but valid encoding would mean two builds of the same
    /// version writing different bytes for the same value, which surfaces much
    /// later as two nodes disagreeing about a snapshot.
    #[test]
    fn the_assembly_packer_agrees_byte_for_byte() {
        if !keva_asm::pack_find::asm_available() {
            // No kernel on this target. That is a supported configuration, not
            // a skipped test — there is simply nothing to diff against.
            return;
        }

        let mut state = 0x2545_F491_4F6C_DD1Du64;
        let mut noise_byte = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state as u8
        };

        let mut cases: Vec<Vec<u8>> = Vec::new();
        for size in [0usize, 1, 3, 4, 5, 7, 8, 15, 16, 17, 64, 1000, 4096, 65_536] {
            cases.push(records(size));
            cases.push(vec![b'a'; size]);
            cases.push((0..size).map(|_| noise_byte()).collect());
        }
        cases.push([b'a', b'b'].repeat(2048));
        let mut edge = b"prefix".to_vec();
        edge.extend(std::iter::repeat(b'z').take(3000));
        edge.extend_from_slice(b"suffix");
        cases.push(edge);

        let mut rust_out = Vec::new();
        let mut asm_out = Vec::new();
        // Both sides start from the sentinel, not from zero: a zero slot reads
        // as position -1 and would send the kernel's verify load out of bounds.
        let mut rust_table = Box::new([EMPTY; HASH_SIZE]);
        let mut asm_table = keva_asm::pack_find::new_table();
        let mut round_trip = Vec::new();

        for input in &cases {
            let rust_kept = pack_with(input, &mut rust_out, &mut rust_table);
            let asm_kept =
                keva_asm::pack_find::pack_asm(input, &mut asm_out, &mut asm_table).is_some();

            assert_eq!(
                rust_kept,
                asm_kept,
                "disagreed on whether packing helped for {} bytes",
                input.len()
            );

            if rust_kept {
                assert_eq!(
                    rust_out,
                    asm_out,
                    "different encodings for the same {} byte input",
                    input.len()
                );
                unpack(&asm_out, &mut round_trip).expect("assembly output must unpack");
                assert_eq!(&round_trip, input, "assembly output lost bytes");
            }
        }
    }

    #[test]
    fn an_offset_before_the_start_is_refused() {
        // Declared length 8, one literal, then a match reaching back further
        // than anything produced.
        let mut stream = Vec::new();
        put_varint_into(8, &mut stream);
        stream.push(1 << 4); // one literal, and a match-length nibble of zero
        stream.push(b'a');
        stream.extend_from_slice(&99u16.to_le_bytes()); // offset far too large

        let mut out = Vec::new();
        assert_eq!(unpack(&stream, &mut out), Err(PackError::BadOffset));
    }
}
