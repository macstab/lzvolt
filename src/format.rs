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
const HASH_BITS: usize = 12;
const HASH_SIZE: usize = 1 << HASH_BITS;

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

#[inline]
fn hash4(bytes: &[u8]) -> usize {
    let word = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    (word.wrapping_mul(0x9E37_79B1) >> (32 - HASH_BITS)) as usize
}

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
    out.clear();
    if input.len() < MIN_MATCH {
        return false;
    }

    put_varint_into(input.len() as u64, out);

    let mut table = [u32::MAX; HASH_SIZE];
    let mut at = 0usize;
    let mut literal_start = 0usize;

    while at + MIN_MATCH <= input.len() {
        let slot = hash4(&input[at..]);
        let candidate = table[slot];
        table[slot] = at as u32;

        let matched = if candidate == u32::MAX {
            0
        } else {
            let candidate = candidate as usize;
            if at - candidate > MAX_OFFSET {
                0
            } else {
                common_prefix(&input[candidate..], &input[at..])
            }
        };

        if matched < MIN_MATCH {
            at += 1;
            continue;
        }

        let candidate = candidate as usize;
        emit_block(&input[literal_start..at], at - candidate, matched, out);

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

fn common_prefix(a: &[u8], b: &[u8]) -> usize {
    let limit = a.len().min(b.len());
    let mut i = 0;
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

/// Unpack into `out`, replacing whatever it held.
///
/// Every length and offset in `input` is treated as hostile: the declared size
/// is capped before anything is reserved, each field is bounds-checked, and a
/// backward reference is verified against what has actually been produced.
pub fn unpack(input: &[u8], out: &mut Vec<u8>) -> Result<(), PackError> {
    out.clear();

    let (declared, header) = get_varint(input).ok_or(PackError::Truncated)?;
    let declared = usize::try_from(declared).map_err(|_| PackError::TooLarge)?;
    if declared > MAX_UNPACKED {
        return Err(PackError::TooLarge);
    }
    out.reserve(declared);

    let mut at = header;
    while out.len() < declared {
        let token = *input.get(at).ok_or(PackError::Truncated)?;
        at += 1;

        let mut literal_len = (token >> 4) as usize;
        if literal_len == 15 {
            literal_len += get_extended(input, &mut at)?;
        }

        let end = at.checked_add(literal_len).ok_or(PackError::TooLarge)?;
        let literals = input.get(at..end).ok_or(PackError::Truncated)?;
        if out.len() + literals.len() > declared {
            return Err(PackError::LengthMismatch);
        }
        out.extend_from_slice(literals);
        at = end;

        if out.len() == declared {
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

        if offset == 0 || offset > out.len() {
            return Err(PackError::BadOffset);
        }
        if out.len() + match_len > declared {
            return Err(PackError::LengthMismatch);
        }

        // Byte at a time on purpose: a match may overlap its own output, which
        // is how a run of one repeated byte is encoded, and `copy_within` would
        // read bytes it has not written yet.
        let start = out.len() - offset;
        for i in 0..match_len {
            let byte = out[start + i];
            out.push(byte);
        }
    }

    if out.len() != declared {
        return Err(PackError::LengthMismatch);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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
