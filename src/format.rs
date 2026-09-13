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
//! [header: varint][block][block]...[switch: 2x2 or 2x4 bytes, if hybrid]
//!
//! header := varint(declared << 2 | hybrid << 1 | split)
//!           split  = 0 -> 4/4 token, 1 -> 2/6 token
//!           hybrid = 1 -> the stream changes split once, see the trailer
//!
//! block  := [token: u8][extended literal length][literals]
//!           [offset: u16][extended match length]
//!           token literal field: 15 (or 3) means "read more"
//!           token match field:   the same, and holds length - 4
//! ```
//!
//! The final block carries literals and no match, which is what terminates the
//! stream — decoding stops when the declared length has been produced.
//!
//! The token's two fields are not fixed at four bits each. `split` chooses
//! between 4/4 and 2/6, and `hybrid` says the stream changes from one to the
//! other partway; the trailer then gives the two positions where that happens,
//! as an offset into the body and the count of bytes produced so far. Both are
//! two bytes for a value up to 64 KiB and four above it, a width that follows
//! from `declared` alone so the decoder knows it before it needs it. See
//! [`Split`] and [`switch_width`].
//!
//! Compatibility with LZ4 runs one way, and both directions are tested.
//! [`our_decoder_reads_what_liblz4_wrote`] passes: an LZ4 block decodes here
//! correctly. [`liblz4_refuses_what_we_wrote`] also passes, and that is the
//! interesting one — the even split *is* the LZ4 token layout, so the body of
//! such a value looks like an LZ4 block. It is not one. LZ4 also constrains
//! where a block may end, and this packer enforces none of that: it runs
//! matches to the final byte and then writes a zero token, which LZ4 has no
//! concept of.
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
/// A second candidate per slot -- keeping the previous occupant and taking
/// whichever gives the longer match -- was built and measured twice, and
/// neither form pays.
///
/// Used wherever it matches, it does not lengthen matches, it creates them:
/// where the newer candidate lost to a collision the older one wins, at a
/// position the search would have walked past. varied_64k went to 2.07x from
/// 1.98x and to 3945 blocks from 3618. Denser and slower to read, which is the
/// trade this format exists to refuse.
///
/// Restricted to positions that already matched, so it can only lengthen, the
/// shape is right -- 3503 blocks at 18.7 bytes each and 2.02x -- and it still
/// does not pay: decoding records_64k fell 11% and packing varied_64k 23%, for
/// two table loads and two verifies on every position visited.
///
/// Three attempts on this axis now, counting reaching two bytes past a match
/// (see [`LAZY_REACH`]). Each moved the ratio and left decoding where it was.
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
///
/// Re-measured against the single-pass packer, because the number above was
/// settled with a method that could not resolve less than 8%. It holds, and for
/// a better reason than cache pressure. On 64 KiB of data that compresses
/// two-fold:
///
/// ```text
///   bits   table    packing    ratio   blocks
///     11   8 KiB   0.65 GiB/s  1.98x     3618
///     12  16 KiB   0.52        2.03x     3976
///     13  32 KiB   0.40        2.08x     4306
/// ```
///
/// A wider table does find more matches -- but they are *short* ones, and a
/// short match is a whole block: a token decode and an offset load for a
/// handful of bytes. Nineteen percent more blocks at thirteen bits is nineteen
/// percent more per-block work on every read, against five percent of ratio
/// once. Decoding is what this store does most, so the wider table is worse on
/// the axis that matters and slower to pack besides.
///
/// This is the same trade liblz4 makes and loses: 2.00x at 4872 blocks and 5.06
/// GiB/s decoding, against our 1.98x at 3618 and 7.27.
///
/// Re-measured again once the hash grew to five bytes, since a hash that stops
/// proposing four-byte matches might have made a wider table affordable. It
/// does not. On 64 KiB of `varied`, cycles per byte and packed bytes:
///
/// ```text
///   bits    11             12             13
///          3.459  33180   3.900  32490   5.018  31968
/// ```
///
/// Eleven still wins by more than a wider table can return in ratio.
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
/// Any value far above the largest reachable position folds the two into one:
/// the search rejects a candidate that is not behind the cursor, and this one
/// never is. No position can collide with it, since a value would have to be
/// two gigabytes long to reach it.
///
/// The table used to hold the position *plus one*, left from when zero was the
/// empty marker, and the search subtracted it back on every position visited --
/// two instructions of the fourteen in the inner loop, for a distinction the
/// sentinel had already made unnecessary.
const EMPTY: u32 = 0x8000_0000;

/// A table slot, so the same pass can be compiled over two widths.
///
/// The profile says the table is where the packer spends its time: on 4 KiB of
/// records, the store into it and the load that stalls behind it are the two
/// largest single instructions in the kernel. A 64-byte line holds sixteen
/// `u32` or thirty-two `u16`, so the same four thousand slots are 256 lines or
/// 128 -- and touched lines are what the cache charges for.
///
/// A position fits in sixteen bits whenever the value does, which the window
/// already bounds at [`MAX_OFFSET`]. The sentinel is the only thing that does
/// not carry over: [`EMPTY`] is far above any reachable position and there is
/// no room for that in sixteen bits. `0xFFFF` serves instead, and is reachable
/// only by a value long enough that this width is not used for it.
///
/// This is liblz4's layout, which holds `u16` entries for an input that fits
/// the window and widens above it.
trait Slot: Copy {
    /// A slot that has never been written.
    const EMPTY: Self;
    /// Largest value this width may be used for. Above it a position would not
    /// fit, or would collide with the sentinel.
    const LIMIT: usize;
    fn position(self) -> usize;
    fn of(at: usize) -> Self;
}

impl Slot for u32 {
    const EMPTY: u32 = EMPTY;
    const LIMIT: usize = usize::MAX;
    #[inline(always)]
    fn position(self) -> usize {
        self as usize
    }
    #[inline(always)]
    fn of(at: usize) -> u32 {
        at as u32
    }
}

impl Slot for u16 {
    const EMPTY: u16 = 0xFFFF;
    /// The search stops at `len - HASH_WORD`, so a value of this length never
    /// reaches position `0xFFFF` and the sentinel stays unambiguous.
    const LIMIT: usize = 0x1_0000;
    #[inline(always)]
    fn position(self) -> usize {
        self as usize
    }
    #[inline(always)]
    fn of(at: usize) -> u16 {
        at as u16
    }
}

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
    /// The position, with [`EMPTY`] for a slot never written.
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
        pack_dispatch(input, out, &mut self.table)
    }
}

/// The kernel where this target has one that implements the production pass,
/// and the portable packer otherwise.
///
/// There is no fallback behind this. The kernel answers 0 for exactly the cases
/// [`pack_with`] answers `false` for -- a value shorter than the hash word, one
/// the probe writes off, and one whose output did not come out smaller -- so a
/// declined pack is an answer, not a request to try again in another language.
/// The byte-for-byte test is what makes that safe to rely on.
///
/// AArch64 is deliberately not routed here. Its kernel implements the eager,
/// fixed-split search rather than this one, so it would pack correctly and
/// differently, and a store whose compressed form depends on which machine
/// wrote it is a store that cannot be replicated.
#[inline]
fn pack_dispatch(input: &[u8], out: &mut Vec<u8>, table: &mut [u32; HASH_SIZE]) -> bool {
    if keva_asm::pack_find::kernel_is_production_packer() {
        return keva_asm::pack_find::pack_asm(input, out, table.as_mut_slice()).is_some();
    }
    pack_with(input, out, table)
}

/// The portable packer, reachable without the kernel taking the call.
///
/// [`Packer`] dispatches to the assembly where a target has it, which is what
/// production wants and what makes the two impossible to compare inside one
/// binary. A profiler needs both in one process: separate builds put the code
/// at different addresses, under different page mappings, with a different
/// heap, and a two-percent difference does not survive that.
///
/// So this is the same pass with the dispatch removed. It is not a fallback and
/// nothing in the store calls it.
#[derive(Debug)]
pub struct PortablePacker {
    table: Box<[u32; HASH_SIZE]>,
}

impl Default for PortablePacker {
    fn default() -> Self {
        PortablePacker {
            table: Box::new([EMPTY; HASH_SIZE]),
        }
    }
}

impl PortablePacker {
    pub fn new() -> PortablePacker {
        PortablePacker::default()
    }

    /// Pack `input` into `out`. See [`pack`] for the contract.
    pub fn pack(&mut self, input: &[u8], out: &mut Vec<u8>) -> bool {
        pack_with(input, out, &mut self.table)
    }
}

/// The same pass over a sixteen-bit table, for measuring that width.
///
/// Not on any production path and deliberately so. Narrowing the table changes
/// nothing about which matches are found -- the positions are identical and so
/// is the output -- but it changes it for whichever packer is narrowed, and the
/// assembly kernel is still thirty-two bits wide. Switching one of them alone
/// would leave the two producing different bytes for the same value, and the
/// test that says they do not is the only reason either is trusted.
///
/// So the width is measured first and migrated afterwards, both sides together.
/// [`Slot::LIMIT`] bounds what this may be handed.
#[derive(Debug)]
pub struct NarrowPacker {
    table: Box<[u16; HASH_SIZE]>,
}

impl Default for NarrowPacker {
    fn default() -> Self {
        NarrowPacker {
            table: Box::new([<u16 as Slot>::EMPTY; HASH_SIZE]),
        }
    }
}

impl NarrowPacker {
    pub fn new() -> NarrowPacker {
        NarrowPacker::default()
    }

    /// Largest value this packer accepts. Above it a position does not fit.
    pub const LIMIT: usize = <u16 as Slot>::LIMIT;

    /// Pack `input` into `out`. Returns `false` for a value this width cannot
    /// index, as well as for one that did not get smaller.
    pub fn pack(&mut self, input: &[u8], out: &mut Vec<u8>) -> bool {
        out.clear();
        if input.len() < HASH_WORD || input.len() > Self::LIMIT {
            return false;
        }
        if !worth_packing(input) {
            return false;
        }
        if input.len() > NARROW_TABLE_ABOVE {
            pack_pass::<{ (64 - HASH_BITS_LARGE) as u32 }, u16>(
                input,
                out,
                &mut self.table,
                EVEN,
                LAZY,
                ADAPTIVE,
            );
        } else {
            pack_pass::<{ (64 - HASH_BITS) as u32 }, u16>(
                input,
                out,
                &mut self.table,
                EVEN,
                LAZY,
                ADAPTIVE,
            );
        }
        out.len() < input.len()
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

/// Hash four bytes the caller has already read.
///
/// It takes the word rather than a slice because the search needs those same
/// four bytes a moment later to reject a candidate, and a slice-taking hash
/// read them a second time.
///
/// That was measured and it is neutral: LLVM already folded the second read
/// away, since the only write between the two is to the table, which it can
/// prove does not alias the input. The redundancy was in the source and never
/// reached the machine code. Kept because saying it once is clearer than
/// relying on the compiler to notice, not because it is faster.
#[inline]
fn hash4(word: u32, shift: u32) -> usize {
    (word.wrapping_mul(0x9E37_79B1) >> shift) as usize
}

/// How far to shift the hash for a value of this length. See
/// [`NARROW_TABLE_ABOVE`].
#[inline]
fn hash_shift(len: usize) -> u32 {
    if len > NARROW_TABLE_ABOVE {
        (64 - HASH_BITS_LARGE) as u32
    } else {
        (64 - HASH_BITS) as u32
    }
}

/// How many bytes the hash reads, which is more than it uses: five bytes are
/// hashed and eight are loaded, because one unaligned 64-bit read is cheaper
/// than assembling five bytes.
const HASH_WORD: usize = 8;

/// The low five bytes at `at`, hashed.
///
/// Both references hash five bytes on a 64-bit target and we hashed four, which
/// was the last thing separating us on data that only partly repeats.
///
/// A four-byte hash proposes every position that shares four bytes, and a
/// four-byte match is a whole block -- a token, an offset, and a literal run cut
/// in two -- bought for nothing. Those are the matches a wider table finds more
/// of, which is why widening ours always made it slower: twelve bits on a 64 KiB
/// value cost 50% and bought 2.6% of ratio. Hashing five bytes does the
/// opposite. It does not propose them at all, so the same table finds fewer and
/// longer matches:
///
/// ```text
///                      four bytes        five bytes
///     varied_64k    3.719   33237 B   3.459   33180 B
///     varied_4k     2.916    2249     2.711    2229
///     records_64k   0.996    6827     0.910    6509
///     records_2k    1.095     392     1.038     394
///     records_512   1.592     192     1.351     195
/// ```
///
/// Faster and denser at once nearly everywhere, and 4.7% denser on 64 KiB of
/// records. The exception is a 512-byte value, where three bytes go the other
/// way: short values have proportionally more four-byte matches worth taking,
/// and there is no room to lose them. Switching the hash on size the way the
/// references switch their table was measured and is worse than either -- the
/// test lands in the inner loop and costs more than it saves, because we cannot
/// compile the loop twice the way a C macro does.
///
/// Wider tables remain wrong even with this: at twelve bits a 64 KiB `varied`
/// value costs 3.900 against 3.459 for 690 bytes of ratio, and at thirteen it
/// is 5.018.
#[inline]
fn hash5(seq: u64, shift: u32) -> usize {
    ((seq << 24).wrapping_mul(889_523_592_379u64) >> shift) as usize
}

/// The eight bytes at `at`, as one word.
///
/// # Safety
///
/// `at + 8 <= input.len()`, which the search bound guarantees.
#[inline]
unsafe fn word8(input: &[u8], at: usize) -> u64 {
    u64::from_le(input.as_ptr().add(at).cast::<u64>().read_unaligned())
}

/// The four bytes at `at`, as one word.
///
/// # Safety
///
/// `at + 4 <= input.len()`. Every caller is inside the search loop, whose
/// condition is `at + MIN_MATCH <= input.len()`, or is reading a candidate
/// position which is strictly behind `at`.
#[inline]
unsafe fn word4(input: &[u8], at: usize) -> u32 {
    // The checked form goes through `try_into` and a `Result`, which profiling
    // found at 3.8% of the packer -- for a bound the loop has already tested.
    u32::from_le(input.as_ptr().add(at).cast::<u32>().read_unaligned())
}

/// How fast the search gives up. After `1 << SKIP_TRIGGER` consecutive misses
/// the cursor starts advancing by more than one byte, and keeps accelerating.
///
/// Without it, data that does not compress is walked one byte at a time to no
/// purpose — and even data that does compress spends most positions between
/// matches. The cost is a slightly worse ratio, since a skipped position is a
/// match never looked for. LZ4 waits for sixty-four misses and we waited with
/// it, which was inherited rather than measured. Thirty-two is better on every
/// shape here, and on two of them it is better on *both* axes -- skipping
/// sooner changes which positions get visited, and on `records` the ones it
/// lands on happen to be worth more:
///
/// ```text
///   misses before skipping        64      32      16
///     noise_512                1.597   1.200   0.910
///     noise_4k                 0.375   0.276   0.202
///     records_512     cycles   1.765   1.598   1.578
///                     bytes      191     192     191
///     records_2k      cycles   1.210   1.079   1.098
///                     bytes      394     392     400
///     records_4k      cycles   1.046   0.962   0.958
///                     bytes      603     601     609
/// ```
///
/// Sixteen is faster still and would halve noise again, but it starts costing
/// ratio on the values a store actually keeps -- one and a half percent on
/// `records_2k`. Density is what this is being sold on and packing happens once
/// per write, so that is the wrong side to spend on.
const SKIP_TRIGGER: usize = 5;

/// How long a match has to be before lazy stops looking for a better one.
///
/// Lazy matching pays for itself when the match in hand is short, because the
/// one a byte later can be much longer. Once it is already long the second
/// search is nearly always wasted: on 64 KiB of `varied` the probe runs 4138
/// times and wins 521, and on `records`, where matches average nineteen bytes,
/// it is worse than that.
///
/// The gate costs one comparison and buys, against no gate at all:
///
/// ```text
///   gate at              none      12      16      20      24
///     records_4k        1.046   0.945   0.962   0.949   0.976
///     records_2k        1.210   1.080   1.079   1.092   1.090
///     records_64k       1.081   0.999   1.009   1.001   1.022
///     varied_64k        3.766   3.509   3.617   3.669   3.748
///     varied_64k    B   33117   33283   33237   33161   33123
/// ```
///
/// Twelve is a shade faster and a shade worse on ratio; sixteen is where the
/// two stop trading against each other. Matches shorter than this are exactly
/// the ones worth improving, and they are the ones the gate lets through.
const LAZY_UNTIL: usize = 16;

/// Write a length that did not fit in a nibble: 255s until a smaller byte.
/// # Safety
///
/// The cursor must have room for the chain; [`Cursor::room`] provides it.
#[inline]
unsafe fn put_extended(mut remaining: usize, out: &mut Cursor) {
    while remaining >= 255 {
        out.byte(255);
        remaining -= 255;
    }
    out.byte(remaining as u8);
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
    pack_dispatch(input, out, &mut table)
}

/// How far the probe reads before writing a value off, and how many slots it
/// reads it into.
///
/// A kilobyte, split across [`PROBE_WINDOWS`] windows, because the question is
/// only whether the value repeats itself at all.
///
/// Eight bits, not more, because the table is zeroed on every call and that is
/// the probe's whole fixed cost -- it returns on one of the first few positions
/// whenever the data repeats, so the memset is what a compressible value
/// actually pays. Halving it from nine bits halves that: with nine, probing
/// costs 2.9% on `records_4k` and 3.1% on `varied_4k`; with eight, 1.6% and
/// 2.0%, at identical output on every shape in the corpus.
///
/// Collisions do not cost accuracy the way they would in the search -- equal
/// words always land in the same slot, and data that compresses repeats often
/// enough that one pair survives a quarter of the slots.
const PROBE: usize = 1024;
const PROBE_BITS: u32 = 8;

/// How many places in the value the probe looks.
///
/// Refusing wrongly costs the whole ratio, not a few percent of throughput, so
/// where the probe reads matters more than how much. Two windows -- head and
/// middle -- judge a value on a tenth of it and miss anything compressible that
/// happens to sit elsewhere, which is not a contrived shape: a buffer written
/// at a high offset, a fixed-size record partly filled, a ciphertext followed
/// by plaintext. Measured on 64 KiB values, packed bytes:
///
/// ```text
///                                    2 windows   4   8   16   unprobed
///   40 KiB of noise then zeros          65536     41276 all the same
///   noise with 30 KiB of zeros mid      65536     35833 all the same
/// ```
///
/// Two windows store both whole. Four recover them completely.
///
/// More than four costs more than it buys, because each window restarts the
/// miss counter and pays its own run-up before the cursor starts skipping.
/// That is charged against incompressible data, which is the case the probe
/// exists for:
///
/// ```text
///   cycles per byte      2       4       8      16   unprobed
///     noise_4k       0.274   0.362   0.471   0.594      0.498
///     noise_64k      0.017   0.022   0.030   0.037      0.156
/// ```
///
/// At eight the probe has nearly stopped paying for itself at 4 KiB, and at
/// sixteen it is worse than not probing at all.
const PROBE_WINDOWS: usize = 4;

/// Only values above this are probed. Below it a full search is a couple of
/// thousand cycles and there is nothing worth saving.
///
/// Two kilobytes rather than four, because four leaves the one shape the probe
/// was built for outside it: a 4 KiB value that does not compress is exactly
/// the size the threshold excluded, and it packs at 0.709 cycles per byte
/// unprobed against 0.362 probed. Rather than one, because a value of exactly
/// two kilobytes then stays outside and keeps its speed -- probing from one
/// kilobyte costs `records_2k` 3.9% and buys nothing it does not already get
/// here.
///
/// ```text
///                unprobed   >4096   >2048   >1024
///   noise_4k        0.709   0.709   0.362   0.371
///   records_2k      1.134   1.134   1.126   1.178
///   records_4k      0.979   0.979   0.995   1.013
///   varied_4k       2.657   2.657   2.711   2.761
/// ```
const PROBE_ABOVE: usize = 2048;

/// Whether the value repeats itself inside its first [`PROBE`] bytes.
///
/// A value that does not is not going to compress, and searching the rest of it
/// produces a stream longer than the input that [`pack_with`] then throws away.
/// That is the shape this packer was furthest behind on: 0.22 cycles per byte
/// spent on 64 KiB of noise to answer a question a kilobyte could answer.
///
/// The probe has its own table and runs before the search, so the search sees
/// exactly the slots it would have seen and the packed bytes are unchanged. Two
/// attempts to answer this inside the search instead -- one testing a byte
/// budget on the miss path, one running the loop to a checkpoint and continuing
/// past it -- both cost 7.5% on compressible 64 KiB, because splitting that
/// loop in two costs more than the branch did.
///
/// It skips on misses exactly as the search does, so it is cheap from both
/// ends: on data that repeats it returns on one of the first few positions, and
/// on data that does not it accelerates away rather than walking the kilobyte.
#[inline(never)]
fn worth_packing(input: &[u8]) -> bool {
    if input.len() <= PROBE_ABOVE {
        return true;
    }
    let mut seen = [0u32; 1 << PROBE_BITS];
    // Windows spread across the value rather than one long one at the front, so
    // it is judged on more than its head. Positions are absolute and the table
    // is shared between windows, so a word in the last window still matches one
    // recorded in the first -- which is what finds a value built from two
    // identical halves.
    let wide = PROBE / PROBE_WINDOWS;
    let step = input.len() / PROBE_WINDOWS;
    for i in 0..PROBE_WINDOWS {
        let start = i * step;
        let last = (start + wide).min(input.len() - MIN_MATCH);
        let mut at = start;
        let mut misses = 1usize << SKIP_TRIGGER;
        while at <= last {
            // SAFETY: `at <= last` is `at + MIN_MATCH <= input.len()`.
            let here = unsafe { word4(input, at) };
            let slot = hash4(here, 32 - PROBE_BITS);
            let stored = seen[slot];
            // Stored one past the position, so an untouched slot is zero and
            // position zero is not mistaken for one.
            seen[slot] = at as u32 + 1;
            // SAFETY: `stored - 1` is a position one of these loops read at.
            if stored != 0 && unsafe { word4(input, stored as usize - 1) } == here {
                return true;
            }
            at += misses >> SKIP_TRIGGER;
            misses += 1;
        }
    }
    false
}

/// The body, with the match table supplied so it can outlive one call.
fn pack_with(input: &[u8], out: &mut Vec<u8>, table: &mut [u32; HASH_SIZE]) -> bool {
    out.clear();
    // Eight, not four: the hash reads eight bytes at every position it visits,
    // so a shorter value has nothing that can be searched. Nothing is lost --
    // a header, a token and an offset already outweigh a seven-byte value.
    if input.len() < HASH_WORD {
        return false;
    }
    if !worth_packing(input) {
        return false;
    }

    // One pass. The split is decided while packing, not by packing twice.
    // Which of the two instantiations, decided once for the whole value.
    if input.len() > NARROW_TABLE_ABOVE {
        pack_pass::<{ (64 - HASH_BITS_LARGE) as u32 }, u32>(
            input, out, table, EVEN, LAZY, ADAPTIVE,
        );
    } else {
        pack_pass::<{ (64 - HASH_BITS) as u32 }, u32>(input, out, table, EVEN, LAZY, ADAPTIVE);
    }
    out.len() < input.len()
}

/// How far past a match the search looks for a longer one.
///
/// One, and the reason two is not better is worth writing down, because the
/// obvious argument says it should be. A longer match is one block instead of
/// two and the decoder is paid by blocks, so reaching further ought to pay
/// twice over. Measured at two:
///
/// ```text
///   varied_64k   3618 -> 3428 blocks   18.1 -> 19.1 B/block   1.98x -> 1.99x
///   decoding                                                   7.51 -> 7.51
///   packing records_64k                                        2.74 -> 2.27
/// ```
///
/// Five percent fewer blocks, five percent more bytes in each, and not one
/// percent of decoding. The block counts moved and the cost per block moved
/// with them: extended match lengths went from 16.8% of blocks to 18.0% and
/// extended literal runs from 15.3% to 17.4%, because a longer match overflows
/// its field and the bytes the step turns loose lengthen the literal run past
/// its own. Every one of those is a chain of 255s -- a dependent load in the
/// decoder's block, which is exactly what the wide split was introduced to
/// remove.
///
/// So bytes per block is not the lever by itself. It is bytes per block *at a
/// fixed cost per block*, and a block pushed over a field boundary is not fixed
/// cost. The wide split worked because it removed chains; the first lazy step
/// worked because it lengthened matches that still fit. This one lengthens them
/// past the fit and gives the gain straight back, and costs 17% of packing to
/// do it.
const LAZY_REACH: usize = 1;

/// Blocks the running window looks back over before changing the split.
///
/// A window rather than a total, because a value is not required to have one
/// character throughout -- a document with a blob in the middle changes shape
/// partway, and a decision taken over the whole value gets such a value wrong
/// no matter which way it goes. Sixteen is short enough that a value of a
/// hundred blocks still spends most of itself in the right split, and long
/// enough that a handful of unusual blocks at the start cannot flip it.
const SPLIT_WINDOW: usize = 16;

/// Whether the pass may change split partway. Off for the reference the
/// assembly packer is diffed against, which has one split and no window.
const ADAPTIVE: bool = true;
#[cfg(test)]
const FIXED: bool = false;

/// Whether the search may give up a byte to look for a longer match.
///
/// A constant with two call sites rather than a setting: the packer always
/// wants it, and the assembly kernel does not have it, so the differential test
/// needs a reference that searches the way the kernel does. Without that the
/// test compares two different algorithms and can only pass by accident -- which
/// is exactly what it did until a `varied` case was added to its corpus.
const LAZY: bool = true;
/// The other value, used only by the differential test below.
#[cfg(test)]
const EAGER: bool = false;

/// The packer as the assembly kernel implements it, for the differential test.
#[cfg(test)]
fn pack_with_eager(input: &[u8], out: &mut Vec<u8>, table: &mut [u32; HASH_SIZE]) -> bool {
    out.clear();
    // Eight, not four: the hash reads eight bytes at every position it visits,
    // so a shorter value has nothing that can be searched. Nothing is lost --
    // a header, a token and an offset already outweigh a seven-byte value.
    if input.len() < HASH_WORD {
        return false;
    }
    if input.len() > NARROW_TABLE_ABOVE {
        pack_pass::<{ (64 - HASH_BITS_LARGE) as u32 }, u32>(input, out, table, EVEN, EAGER, FIXED);
    } else {
        pack_pass::<{ (64 - HASH_BITS) as u32 }, u32>(input, out, table, EVEN, EAGER, FIXED);
    }
    out.len() < input.len()
}

/// One pass, deciding the split as it goes.
///
/// The header is written first and patched last, which works because its length
/// depends only on the declared size -- known at the first byte -- and not on
/// the two flag bits in it. So the pass commits to nothing up front: it starts
/// in `split`, watches a running window of how often the match field overflows,
/// and if it does so in more than half of the last [`SPLIT_WINDOW`] blocks it
/// changes split there and then, records where, and sets the bit at the end.
///
/// This replaces packing the value twice and throwing the first away. It also
/// answers a question two passes could not: which split fits *this part* of the
/// value. A sample can be unrepresentative and a total cannot see a change of
/// character; a window sees both.
fn pack_pass<const SHIFT: u32, S: Slot>(
    input: &[u8],
    out: &mut Vec<u8>,
    table: &mut [S; HASH_SIZE],
    start_split: Split,
    lazy: bool,
    adaptive: bool,
) {
    // Written with the flag bits clear. Their value cannot be known yet, and
    // does not need to be: the varint's length is fixed by `input.len()`.
    put_varint_into(((input.len() as u64) << 2) | start_split.bit(), out);
    let header_len = out.len();
    // Reserved once, so nothing below has to ask again -- and asked about
    // first, because the caller reuses its buffer and growth is the case that
    // never happens. `Vec::reserve` is a call; a capacity compare is not.
    let want = Cursor::room(input.len());
    if out.capacity() - out.len() < want {
        out.reserve(want);
    }
    // SAFETY: the reserve above is exactly what the cursor's writes assume.
    let mut cur = unsafe { Cursor::new(out) };
    let mut split = start_split;
    // Where the second section starts, as (offset into the body, output
    // position). Both are needed: the decoder splits the input at the first and
    // resumes producing at the second.
    let mut switch: Option<(usize, usize)> = None;
    let mut recent: u32 = 0;
    let mut seen = 0usize;

    let mut blocks = 0usize;
    let mut saturated = 0usize;
    let mut at = 0usize;
    let mut literal_start = 0usize;
    // Starts at the trigger, so the first step is one byte. Starting at one
    // would make the shift zero and the cursor would sit on the same position
    // for the first sixty-four attempts.
    let mut misses = 1usize << SKIP_TRIGGER;
    // One probe ahead of `at`, so a miss hands over an address that was formed
    // a loop earlier. Re-seeded from `at` wherever the cursor jumps for another
    // reason.
    let mut next_at = at;
    // The shift is a constant here, not a value, and that is worth a whole
    // instruction per position. It takes exactly two values -- 64 - HASH_BITS
    // or 64 - HASH_BITS_LARGE -- chosen once from the length, but x86 has no
    // register-to-register variable shift outside BMI2: the count has to be in
    // CL, and CL is wanted two instructions later, so the generated code
    // reloaded it every single iteration:
    //
    //     imulq %r11, %rsi
    //     movl  %r13d, %ecx      <- every position, for a loop invariant
    //     shrq  %cl, %rsi
    //
    // As a constant it is `shrq $52` and CL stays free. AArch64 shifts by a
    // register in one instruction and neither gains nor loses.
    debug_assert_eq!(SHIFT, hash_shift(input.len()));

    // Hoisted: the loop asked `at + MIN_MATCH <= input.len()` and paid an `add`
    // for it on every position visited.
    // SAFETY of every `word8` below: the callers refuse a value shorter than
    // `HASH_WORD`, so this does not wrap and the last position visited has a
    // full hash word above it.
    let last = input.len() - HASH_WORD;
    loop {
        // The advance sits before the probe, not after it, which is what makes
        // it a lookahead rather than a rename: the address this iteration loads
        // from was formed last iteration, so it does not wait on the branch
        // that closed the last one. lz4_flex writes the same two lines at the
        // same place.
        at = next_at;
        next_at = at + (misses >> SKIP_TRIGGER);
        if at > last {
            break;
        }
        // One load, not two. These were written as `word4(at)` and `word8(at)`
        // and the generated x86 kept them apart:
        //
        //     movl  (%r15,%r14), %esi        ; word4
        //     movq  (%r15,%r14), %rax        ; word8, same address
        //
        // Both are little-endian reads at the same address, so the four bytes
        // are the low half of the eight and taking them from the register is
        // exact. That is a load and an address-generation slot per position, on
        // the path data that does not repeat spends nearly all of its time in.
        //
        // SAFETY: `at <= last` is `at + 8 <= input.len()`, which covers both.
        let word = unsafe { word8(input, at) };
        let here = word as u32;
        let slot = hash5(word, SHIFT);
        let stored = table[slot];
        table[slot] = S::of(at);

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
        // Two tests, and they stay two.
        //
        // "Behind the cursor" and "inside the window" are one question asked of
        // the distance -- `(at - candidate - 1) < MAX_OFFSET` in unsigned
        // arithmetic covers both, since an empty or ahead slot wraps to
        // something enormous. That was built and measured: noise_4k improved 3%
        // and everything else got worse, varied_64k by 5%, varied_4k by 4%,
        // records_4k by 3%. Three runs each.
        //
        // The reason is the short circuit. Once the table is warm the first
        // test is almost always true and almost always predicted, so the second
        // is cheap; folding them makes both arithmetic on every position
        // whether or not it is needed. Only on data that does not compress,
        // where the table holds stale entries and neither test predicts, does
        // the fold pay.
        //
        // Two candidates for a different search were built. Neither wins.
        //
        // Folding the two tests into `(at - candidate - 1) < MAX_OFFSET`,
        // measured back to back against this in one sitting: noise_4k 0.748 ->
        // 0.734 cycles per byte, varied_64k 3.853 -> 4.331, records_4k 1.014 ->
        // 1.024. Two percent on the target, twelve against everywhere else.
        //
        // Reading the candidate unconditionally and branching only on whether
        // four bytes agree -- which is what liblz4 does, and why it executes
        // more instructions per byte than we do: noise_4k flat, varied_64k 11%
        // worse, worse on every other shape. The unconditional read costs more
        // where the branch would have predicted than it saves where it would
        // not, and predictable positions are the overwhelming majority
        // everywhere except noise.
        //
        // A second search remains the right idea -- which search found a match
        // is invisible to the decoder, so switching costs nothing in the format
        // and could ride the miss streak the way the split rides the saturation
        // window. What is missing is a second search that wins on its own
        // target. These two do not.
        //
        // Which is where the real gap is, and it is not this. Profiling both
        // packers over the same 4 KiB of noise:
        //
        //     keva     0.740 cycles/byte   IPC 3.13   2.3 instr/byte   16.5% mispredicts
        //     liblz4   0.539               IPC 6.12   3.3             2.1%
        //
        // liblz4 executes half again as many instructions per byte and is 27%
        // faster, at twice the issue rate, because it loses almost nothing to
        // misprediction. Ours is a search whose branches depend on what the
        // table happens to hold; theirs is shaped so that they do not. Closing
        // that is a different loop, not a smaller one.
        let candidate = stored.position();
        let matched = if candidate < at
            && at - candidate <= MAX_OFFSET
            // SAFETY: `candidate < at`, and `at + MIN_MATCH <= input.len()`.
            && unsafe { word4(input, candidate) } == here
        {
            // SAFETY: same, and `candidate + MIN_MATCH < at + MIN_MATCH`.
            MIN_MATCH + unsafe { common_prefix(input, candidate + MIN_MATCH, at + MIN_MATCH) }
        } else {
            0
        };

        if matched < MIN_MATCH {
            // The cursor for the next probe is formed here rather than being
            // derived from this one at the top, which is what lz4_flex does:
            //
            //     cur = next_cur;
            //     next_cur += step_size;
            //
            // Same arithmetic, one iteration earlier. The address the next
            // probe loads from stops depending on this iteration reaching its
            // end, so the load can issue while the rest of this block is still
            // in flight instead of after the branch that closes it.
            misses += 1;
            continue;
        }
        misses = 1 << SKIP_TRIGGER;

        // The first match found is not always the one worth taking.
        //
        // The search moves forward and commits to whatever it finds, so a
        // four-byte match at one position hides a forty-byte match starting a
        // byte later. Giving up the byte to the literal run costs one byte of
        // ratio and buys the difference between the two lengths.
        //
        // This matters here more than it would in an archiver, because block
        // count is what the decoder is paid by: a longer match is not just
        // denser, it is one block instead of two, and the two would each have
        // cost a token decode and an offset load. Counting real blocks put us
        // 25% above lz4_flex on the same data at the same ratio, and this is
        // that difference.
        //
        // The step repeats while it keeps winning, which terminates because
        // every round demands a strictly longer match than the last.
        let mut candidate = candidate;
        let mut matched = matched;
        // A guard on the outside and a loop on the inside, because `lazy` never
        // changes and `while lazy` reads as though it might.
        if lazy && matched < LAZY_UNTIL {
            'lazy: loop {
                // One byte on, and if that finds nothing better, two.
                //
                // Stepping only by one gives up as soon as the very next position
                // fails, which loses a long match sitting two bytes away -- and a
                // long match is what the decoder is paid by, since it is one block
                // instead of two. The second probe costs a hash and a lookup on a
                // position the search would otherwise have skipped, and the entry it
                // leaves behind is one a later search can use.
                for step in 1..=LAZY_REACH {
                    let next = at + step;
                    if next > last {
                        break 'lazy;
                    }
                    // SAFETY: `next + MIN_MATCH <= input.len()` was tested above.
                    let ahead = unsafe { word4(input, next) };
                    // SAFETY: `next <= last` is `next + 8 <= input.len()`.
                    let slot = hash5(unsafe { word8(input, next) }, SHIFT);
                    let stored = table[slot];
                    // Inserted whether or not it wins: the position is real and a
                    // later search may want it, and the search itself would never
                    // have visited it.
                    table[slot] = S::of(next);

                    let other = stored.position();
                    if other < next
                        && next - other <= MAX_OFFSET
                        // SAFETY: `other < next` and `next + MIN_MATCH <= input.len()`.
                        && unsafe { word4(input, other) } == ahead
                    {
                        let len = MIN_MATCH
                            + unsafe { common_prefix(input, other + MIN_MATCH, next + MIN_MATCH) };
                        // Strictly longer, and by enough to pay for the literals the
                        // step turns loose: moving `step` bytes out of a match and
                        // into the literal run costs about that many bytes.
                        if len > matched + step - 1 {
                            matched = len;
                            candidate = other;
                            at = next;
                            continue 'lazy;
                        }
                    }
                }
                break;
            }
        }

        // The bounds checks in this loop stay, and that is measured rather than
        // assumed. In the disassembly it is eleven instructions per byte walked
        // where six do the work, with a range check per iteration whose answer
        // the loop conditions already guarantee. Hoisting them into an unsafe
        // helper was tried twice: folding the two conditions into a `min` up
        // front cost 4.8% on varied_64k for 2 to 4% on records, and keeping the
        // short circuit while dropping only the checks was worse than either --
        // 4.72 cycles a byte against 4.42. Three runs each.
        //
        // The reason the disassembly misleads is that this loop usually runs
        // zero times: a match found by a forward search rarely extends
        // backwards on data that does not repeat, so what the body costs per
        // iteration hardly matters and disturbing the code around it does.
        //
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

        blocks += 1;
        let overflowed = matched + back - MIN_MATCH >= split.mat_max();
        if overflowed {
            saturated += 1;
        }
        // The decision, taken before this block is written so that the block
        // itself already uses the split it argued for. `literal_start` is the
        // output position every emitted block has produced so far, and the body
        // offset is simply how much has been written past the header.
        if adaptive && switch.is_none() && split == EVEN {
            recent = (recent << 1) | overflowed as u32;
            seen += 1;
            let mask = (1u32 << SPLIT_WINDOW) - 1;
            let fits = |v: usize| v < 1 << (8 * switch_width(input.len()));
            if seen >= SPLIT_WINDOW
                && (recent & mask).count_ones() as usize * 2 > SPLIT_WINDOW
                // The window sits at the front of the value, so this holds in
                // practice; it is checked rather than argued because the
                // trailer's width cannot represent more.
                && fits(cur.len() - header_len)
                && fits(literal_start)
            {
                switch = Some((cur.len() - header_len, literal_start));
                split = LONG_MATCH;
            }
        }
        let literals = &input[literal_start..at - back];
        let wide = wide_literals_fit(literals, at - back, input.len());
        emit_block(
            literals,
            at - candidate,
            matched + back,
            &mut cur,
            split,
            wide,
        );

        at += matched;
        next_at = at;
        literal_start = at;
    }

    // Everything left over is literals, with no match to follow.
    emit_literals_only(&input[literal_start..], &mut cur, split);
    // SAFETY: every byte below the cursor was written by the calls above.
    unsafe { out.set_len(cur.len()) };

    if let Some((in_at, out_at)) = switch {
        // At the end rather than in the header, so a value that never switches
        // pays nothing for the possibility. Fixed width because a varint cannot
        // be read backwards without ambiguity -- its continuation bits look no
        // different from a body byte's high bit.
        let w = switch_width(input.len());
        put_switch(in_at, w, out);
        put_switch(out_at, w, out);
        // The low bits live in the first byte whatever the varint's length.
        out[0] |= HYBRID_BIT as u8;
    }
    let _ = (blocks, saturated);
}

/// Header bit 0: which split the first section uses.
/// Header bit 1: whether there is a second section.
const HYBRID_BIT: u64 = 0b10;

/// Bytes each of the two switch offsets takes at the end of a hybrid stream.
///
/// Both are bounded by the value: the output position is below `declared` and
/// the body offset below the body, which is shorter still. So a value that fits
/// in sixteen bits needs two bytes per field rather than four, and since the
/// window is fixed at the front of the value the offsets are small whatever the
/// value's size.
///
/// The width follows from `declared` alone, which the decoder reads before it
/// needs the trailer. It is not a flag, because a flag would be another bit to
/// get wrong.
///
/// This is worth being careful about. Measured against the two-pass packer, the
/// eight-byte trailer *was* the entire ratio cost of switching -- 272 to 279
/// bytes at 1 KiB, 599 to 607 at 4 KiB, 6784 to 6791 at 64 KiB. The prefix left
/// in the assumed split costs nothing; it comes out a byte ahead.
#[inline]
fn switch_width(declared: usize) -> usize {
    if declared <= 0x1_0000 {
        2
    } else {
        4
    }
}

#[inline]
fn put_switch(value: usize, width: usize, out: &mut Vec<u8>) {
    match width {
        2 => out.extend_from_slice(&(value as u16).to_le_bytes()),
        _ => out.extend_from_slice(&(value as u32).to_le_bytes()),
    }
}

#[inline]
fn get_switch(bytes: &[u8]) -> usize {
    match bytes.len() {
        2 => u16::from_le_bytes(bytes.try_into().unwrap()) as usize,
        _ => u32::from_le_bytes(bytes.try_into().unwrap()) as usize,
    }
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
/// Length of the run shared by `input[a..]` and `input[b..]`, with `a < b`.
///
/// Takes positions rather than two slices. Building the slices cost two range
/// checks and a `min` of their lengths on every match found -- profiling put
/// 3.8% of the packer in that `min` alone -- and the caller already knows both
/// facts: the later position bounds the comparison, and it is the later one by
/// construction.
///
/// # Safety
///
/// `a < b` and `b <= input.len()`.
unsafe fn common_prefix(input: &[u8], a: usize, b: usize) -> usize {
    let limit = input.len() - b;
    let base = input.as_ptr();
    let mut i = 0;

    while i + 8 <= limit {
        // SAFETY: `b + i + 8 <= input.len()` from the loop condition, and
        // `a < b`, so both reads are inside the slice.
        let left = u64::from_le(base.add(a + i).cast::<u64>().read_unaligned());
        let right = u64::from_le(base.add(b + i).cast::<u64>().read_unaligned());
        let diff = left ^ right;
        if diff != 0 {
            // Little-endian, so the first differing byte is the lowest set bit.
            return i + (diff.trailing_zeros() / 8) as usize;
        }
        i += 8;
    }

    while i < limit && *base.add(a + i) == *base.add(b + i) {
        i += 1;
    }
    i
}

/// How the token's eight bits are split between the two lengths.
///
/// Four and four is LZ4's, and it is what a block liblz4 wrote uses. It is not
/// what our own data wants: counting the lengths a real value decomposes into
/// says the match length overflows its four bits in 99% of blocks on data that
/// compresses six-fold, because the average match there is 34 bytes. Six bits
/// take that to zero, and the two bits it leaves for literals are enough
/// because the same data averages 0.4 to 2.1 of them.
///
/// Data that compresses only two-fold wants the opposite: its matches never
/// overflow four bits, and its literal runs average five to six, so narrowing
/// the literal field to two bits would overflow 86% of them. One split cannot
/// serve both, so the packer picks per value and records the choice in the
/// header.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Split {
    lit_bits: u32,
}

/// Four bits each, which is also the LZ4 block format.
const EVEN: Split = Split { lit_bits: 4 };
/// Two for the literal run, six for the match.
const LONG_MATCH: Split = Split { lit_bits: 2 };

impl Split {
    #[inline]
    fn lit_max(self) -> usize {
        (1 << self.lit_bits) - 1
    }
    #[inline]
    fn mat_max(self) -> usize {
        (1 << (8 - self.lit_bits)) - 1
    }
    #[inline]
    fn shift(self) -> u32 {
        8 - self.lit_bits
    }
    #[inline]
    fn from_header(bit: u64) -> Split {
        if bit & 1 == 1 {
            LONG_MATCH
        } else {
            EVEN
        }
    }
    /// The other of the two splits.
    #[inline]
    fn other(self) -> Split {
        if self == EVEN {
            LONG_MATCH
        } else {
            EVEN
        }
    }
    #[inline]
    fn bit(self) -> u64 {
        (self.lit_bits == LONG_MATCH.lit_bits) as u64
    }
    #[inline]
    fn kernel(self) -> keva_asm::unpack::Split {
        if self == LONG_MATCH {
            keva_asm::unpack::Split::WideMatch
        } else {
            keva_asm::unpack::Split::Even
        }
    }
}

/// Bytes the fixed-width literal move writes, whatever the run's true length.
///
/// `extend_from_slice` compiles to a call to `memmove` with a length the
/// compiler cannot see, and a literal run averages six bytes -- so the call
/// costs more than the copy. Profiling found 19.5% of the packer there. A move
/// of constant width has no call in it; it reads past the run, so the caller
/// shows that those bytes are inside the input, and it writes past the output,
/// which the reserve covers.
const WIDE_LITERAL: usize = 16;

/// Whether a literal run ending at `end` can be copied the wide way.
///
/// The run is a slice of the input, so reading a fixed width past its end has
/// to stay inside the input.
#[inline]
fn wide_literals_fit(literals: &[u8], end: usize, input_len: usize) -> bool {
    literals.len() <= WIDE_LITERAL && end + WIDE_LITERAL <= input_len
}

/// A write cursor into the output, past the point where its size is in doubt.
///
/// Every `push` and `extend_from_slice` in the block writer carries a capacity
/// check, and profiling put 9.4% of the packer in that bookkeeping -- for a
/// buffer whose worst case is known before the pass starts.
///
/// The bound is the whole input as literals, plus a token and an offset for
/// each block, plus the chains of 255s, plus the overshoot the fixed-width
/// literal move makes. A block covers at least [`MIN_MATCH`] output bytes, so
/// there are at most `n / MIN_MATCH` of them. In a debug build every write
/// checks itself against the reserve, so an overrun is a failing test rather
/// than a corrupted heap.
struct Cursor {
    p: *mut u8,
    base: *mut u8,
    #[cfg(debug_assertions)]
    limit: *mut u8,
}

impl Cursor {
    #[inline]
    fn room(n: usize) -> usize {
        n + n / MIN_MATCH * 3 + n / 128 + WIDE_LITERAL + 64
    }

    /// # Safety
    ///
    /// `out` must have [`Cursor::room`] spare capacity past its length.
    unsafe fn new(out: &mut Vec<u8>) -> Cursor {
        let base = out.as_mut_ptr();
        Cursor {
            p: base.add(out.len()),
            base,
            #[cfg(debug_assertions)]
            limit: base.add(out.capacity()),
        }
    }

    #[inline]
    fn guard(&self, _n: usize) {
        #[cfg(debug_assertions)]
        debug_assert!(
            unsafe { self.p.add(_n) } <= self.limit,
            "the packer wrote past its reserve"
        );
    }

    #[inline]
    unsafe fn byte(&mut self, b: u8) {
        self.guard(1);
        *self.p = b;
        self.p = self.p.add(1);
    }

    #[inline]
    unsafe fn offset(&mut self, v: u16) {
        self.guard(2);
        self.p.cast::<u16>().write_unaligned(v.to_le());
        self.p = self.p.add(2);
    }

    /// Copies a fixed width and advances by the true length. See
    /// [`WIDE_LITERAL`].
    #[inline]
    unsafe fn literals_wide(&mut self, src: &[u8]) {
        self.guard(WIDE_LITERAL);
        std::ptr::copy_nonoverlapping(src.as_ptr(), self.p, WIDE_LITERAL);
        self.p = self.p.add(src.len());
    }

    #[inline]
    unsafe fn literals(&mut self, src: &[u8]) {
        self.guard(src.len());
        std::ptr::copy_nonoverlapping(src.as_ptr(), self.p, src.len());
        self.p = self.p.add(src.len());
    }

    #[inline]
    fn len(&self) -> usize {
        // SAFETY: `p` never moves below `base`.
        unsafe { self.p.offset_from(self.base) as usize }
    }
}

fn emit_block(
    literals: &[u8],
    offset: usize,
    matched: usize,
    out: &mut Cursor,
    split: Split,
    wide: bool,
) {
    let match_extra = matched - MIN_MATCH;
    let lit_field = literals.len().min(split.lit_max());
    let mat_field = match_extra.min(split.mat_max());
    // SAFETY: the pass reserved `Cursor::room` before the first block, and a
    // debug build checks every write against it.
    unsafe {
        out.byte(((lit_field as u8) << split.shift()) | mat_field as u8);
        if literals.len() >= split.lit_max() {
            put_extended(literals.len() - split.lit_max(), out);
        }
        if wide {
            out.literals_wide(literals);
        } else {
            out.literals(literals);
        }
        out.offset(offset as u16);
        if match_extra >= split.mat_max() {
            put_extended(match_extra - split.mat_max(), out);
        }
    }
}

fn emit_literals_only(literals: &[u8], out: &mut Cursor, split: Split) {
    let lit_field = literals.len().min(split.lit_max());
    // SAFETY: as in `emit_block`.
    unsafe {
        // A zero match field with no offset following is what marks the tail;
        // the decoder knows to stop because the declared length has been
        // reached.
        out.byte((lit_field as u8) << split.shift());
        if literals.len() >= split.lit_max() {
            put_extended(literals.len() - split.lit_max(), out);
        }
        out.literals(literals);
    }
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
    // The kernel is handed a body and a length and answers one question: did it
    // decode. Declining is not an error report — it means "this needs the
    // decoder that can name what is wrong", which is the one below. So a stream
    // the kernel refuses is never rejected on its word alone.
    if keva_asm::unpack::asm_available() {
        if let Ok(f) = frame(input) {
            let ok = match f.switch {
                None => keva_asm::unpack::unpack_asm(f.body, out, f.declared, f.split.kernel()),
                Some(switch) => keva_asm::unpack::unpack_asm_hybrid(
                    f.body,
                    out,
                    f.declared,
                    switch,
                    f.split.kernel(),
                    f.split.other().kernel(),
                ),
            };
            if ok {
                return Ok(());
            }
        }
    }
    unpack_portable(input, out)
}

/// Unpack a block whose uncompressed length the caller already knows.
///
/// This is the shape LZ4's own API has -- `LZ4_decompress_safe` is handed the
/// destination and its size, because an LZ4 block carries no length of its own.
/// [`unpack`] reads ours from the varint in front of the stream; this skips it.
///
/// The compatibility runs one way only, and it is worth being exact about
/// which. **Reading**: this decoder accepts any block liblz4 wrote, byte for
/// byte, which the interop test checks — the even split is the LZ4 token
/// layout, extended lengths are the same chains of 255, the offset is the same
/// two little-endian bytes, and match lengths are stored less four. That is
/// what makes a decoder-against-decoder measurement possible with no format
/// difference left in it.
///
/// **Writing**: our packer's output is not an LZ4 block and liblz4 refuses it.
/// The layout matches; the constraints do not. LZ4 requires a block to end in a
/// literal run of at least five bytes with no match inside the last twelve,
/// while our packer emits matches up to the final byte and then a zero token to
/// close. Half our values also choose the wide split, whose token is not LZ4's
/// at all. See `liblz4_refuses_what_we_wrote` for the measurement.
pub fn unpack_into(block: &[u8], out: &mut Vec<u8>, declared: usize) -> Result<(), PackError> {
    if declared > MAX_UNPACKED {
        return Err(PackError::TooLarge);
    }
    // The LZ4 body, not our own. Which of the two runs is decided here and
    // nowhere else, by the shape of the call rather than by looking at the
    // bytes: a caller who has to hand over a length is holding a block that
    // does not carry one, and only LZ4 blocks do not carry one.
    if keva_asm::unpack::asm_available() && keva_asm::unpack::unpack_lz4_asm(block, out, declared) {
        return Ok(());
    }
    // The portable decoder wants the header, so give it one.
    let mut framed = Vec::with_capacity(block.len() + 10);
    put_varint_into(((declared as u64) << 2) | EVEN.bit(), &mut framed);
    framed.extend_from_slice(block);
    unpack_portable(&framed, out)
}

/// What a packed value's frame says: the declared size, the body, and where the
/// split changes if it does.
struct Frame<'a> {
    declared: usize,
    body: &'a [u8],
    split: Split,
    /// `(offset into body, output position)` where the second section starts.
    switch: Option<(usize, usize)>,
}

/// Read the header, and the trailer if the header says there is one.
///
/// Both offsets come off the wire. They are range-checked here so that
/// everything downstream can treat them as ordinary positions, and anything
/// inconsistent is a truncated stream rather than a silent mis-decode.
fn frame(input: &[u8]) -> Result<Frame<'_>, PackError> {
    let (raw, header) = get_varint(input).ok_or(PackError::Truncated)?;
    let declared = usize::try_from(raw >> 2).map_err(|_| PackError::TooLarge)?;
    if declared > MAX_UNPACKED {
        return Err(PackError::TooLarge);
    }
    let split = Split::from_header(raw);
    let rest = input.get(header..).ok_or(PackError::Truncated)?;

    if raw & HYBRID_BIT == 0 {
        return Ok(Frame {
            declared,
            body: rest,
            split,
            switch: None,
        });
    }

    let w = switch_width(declared);
    let cut = rest.len().checked_sub(2 * w).ok_or(PackError::Truncated)?;
    let (body, tail) = rest.split_at(cut);
    let in_at = get_switch(&tail[..w]);
    let out_at = get_switch(&tail[w..]);
    if in_at > body.len() || out_at == 0 || out_at >= declared {
        return Err(PackError::Truncated);
    }
    Ok(Frame {
        declared,
        body,
        split,
        switch: Some((in_at, out_at)),
    })
}

/// The decoder every kernel is diffed against, and the only place an error is
/// given a name.
fn unpack_portable(input: &[u8], out: &mut Vec<u8>) -> Result<(), PackError> {
    out.clear();

    let f = frame(input)?;
    let (declared, body) = (f.declared, f.body);
    let mut split = f.split;
    let switch_at = f.switch.map(|(_, out_at)| out_at);
    out.reserve(declared + OVERRUN);

    let capacity = out.capacity();
    let base = out.as_mut_ptr();
    let mut produced = 0usize;
    let mut at = 0usize;
    let input = body;

    while produced < declared {
        // The second section begins on a block boundary, so this is the only
        // place the split can change. A stream that steps over the boundary
        // instead of landing on it decodes the rest with the wrong split and
        // fails the length check below, which is the same outcome as any other
        // corruption.
        if switch_at == Some(produced) {
            split = split.other();
        }
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
        let short_literal = (token >> split.shift()) as usize;
        let short_match = (token & split.mat_max() as u8) as usize;
        // The fixed-size match copy below moves one OVERRUN block, so a match
        // longer than that belongs on the exact path. Under the even split no
        // short match can reach it -- fourteen plus four is eighteen -- but the
        // wide split carries them to sixty-seven.
        if short_literal != split.lit_max()
            && short_match != split.mat_max()
            && short_match + MIN_MATCH <= OVERRUN
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

        let mut literal_len = (token >> split.shift()) as usize;
        if literal_len == split.lit_max() {
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

        let mut match_len = (token & split.mat_max() as u8) as usize;
        if match_len == split.mat_max() {
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

        // A believable declared length in front of noise, in every combination
        // of the two header flags -- a stream claiming a second section it does
        // not have has to be refused like any other corruption.
        for len in [0u64, 1, 64, 4096, 1 << 20] {
            for flags in 0u64..4 {
                for _ in 0..200 {
                    let mut stream = Vec::new();
                    put_varint_into((len << 2) | flags, &mut stream);
                    for _ in 0..(next() % 200) {
                        stream.push(next() as u8);
                    }
                    let _ = unpack(&stream, &mut out);
                }
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
    /// Drive both decoders over everything the packer can emit, and require
    /// that they produce the same bytes.
    ///
    /// The sizes are chosen around the widths the kernel copies in: sixteen for
    /// a short literal run, thirty-two for a block. The repeating patterns are
    /// there for the other reason — a two- or three-byte offset takes the
    /// doubling path, which is the only part of the decoder that reads bytes it
    /// wrote moments earlier, and it is where an off-by-one would corrupt data
    /// rather than crash.
    /// The reference implementation as a third opinion.
    ///
    /// Two decoders written by one author can share one misreading of the
    /// format; a third that predates both cannot. liblz4 compresses, we decode,
    /// and the bytes have to come back — which also establishes the claim the
    /// benchmark rests on, that our decoder reads a genuine LZ4 block and a
    /// decoder-against-decoder measurement has no format difference left in it.
    ///
    /// Off by default because it links a C library:
    ///   RUSTFLAGS="-L/opt/homebrew/lib" cargo test -p keva-core --features liblz4
    #[cfg(feature = "liblz4")]
    #[test]
    fn our_decoder_reads_what_liblz4_wrote() {
        #[link(name = "lz4")]
        extern "C" {
            fn LZ4_compress_default(src: *const u8, dst: *mut u8, n: i32, cap: i32) -> i32;
        }

        let mut state = 0x9E37_79B9_7F4A_7C15u64;
        let mut noise_byte = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state as u8
        };

        let mut cases: Vec<Vec<u8>> = Vec::new();
        for size in [1usize, 4, 15, 16, 17, 63, 64, 65, 1000, 4096, 65_536] {
            cases.push(records(size));
            cases.push(vec![b'a'; size]);
            cases.push((0..size).map(|_| noise_byte()).collect());
        }
        cases.push(b"ab".repeat(3000));
        cases.push(b"abcdefg".repeat(900));

        let mut out = Vec::new();
        for data in &cases {
            let mut block = vec![0u8; data.len() + 1024];
            let n = unsafe {
                LZ4_compress_default(
                    data.as_ptr(),
                    block.as_mut_ptr(),
                    data.len() as i32,
                    block.len() as i32,
                )
            };
            assert!(n > 0, "liblz4 refused {} bytes", data.len());
            block.truncate(n as usize);

            unpack_into(&block, &mut out, data.len())
                .unwrap_or_else(|e| panic!("{} bytes from liblz4: {e}", data.len()));
            assert_eq!(&out[..], &data[..], "differs on {} bytes", data.len());
        }
    }

    /// A narrower table must find the same matches, not merely valid ones.
    ///
    /// The whole argument for sixteen-bit slots is that they change nothing
    /// except how much cache the table occupies: the positions are the same
    /// numbers, so the same candidates are proposed and the same bytes come
    /// out. If that is wrong the width is not a free optimisation but a format
    /// change, and this is what says which.
    ///
    /// It also pins the sentinel. `0xFFFF` stands for an untouched slot, and a
    /// value long enough to reach position 65535 would make that ambiguous --
    /// [`NarrowPacker::LIMIT`] is where that line sits, and the boundary
    /// lengths below are on both sides of it.
    #[test]
    fn a_narrow_table_packs_the_same_bytes() {
        let mut state = 0x2545_F491_4F6C_DD1Du64;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };

        let mut cases: Vec<Vec<u8>> = Vec::new();
        for size in [
            8usize, 64, 512, 2048, 2049, 4096, 8192, 8193, 40_000, 65_536,
        ] {
            cases.push(records(size));
            cases.push(vec![b'a'; size]);
            cases.push((0..size).map(|_| next() as u8).collect());
        }

        let mut wide_out = Vec::new();
        let mut narrow_out = Vec::new();
        let mut wide = PortablePacker::new();
        let mut narrow = NarrowPacker::new();

        let mut compared = 0usize;
        for input in &cases {
            if input.len() > NarrowPacker::LIMIT {
                continue;
            }
            let a = wide.pack(input, &mut wide_out);
            let b = narrow.pack(input, &mut narrow_out);
            assert_eq!(
                a,
                b,
                "the two widths disagreed on whether packing helped for {} bytes",
                input.len()
            );
            if a {
                assert_eq!(
                    wide_out,
                    narrow_out,
                    "a {}-byte value packs differently at sixteen bits",
                    input.len()
                );
                compared += 1;
            }
        }
        assert!(compared > 10, "only {compared} cases actually packed");
    }

    /// A block that is one literal run and nothing else, asked of the kernel
    /// directly.
    ///
    /// This is the shape liblz4 writes for data that does not compress, and it
    /// needs no liblz4 to construct: a token carrying the literal count, the
    /// chain of 255s when it does not fit, and then the bytes. It is the same
    /// encoding [`emit_literals_only`] produces, which is why it can be built
    /// here.
    ///
    /// Worth its own test because it is the one block shape our own packer
    /// never stores -- a value that does not compress is kept raw, so the only
    /// way this reaches the decoder is a store migrating from LZ4. Everything
    /// covering that path goes through `unpack_into`, which falls back to the
    /// portable decoder without saying so.
    #[test]
    fn the_kernel_reads_a_block_that_is_all_literals() {
        if !keva_asm::unpack::asm_available() {
            return;
        }

        let mut state = 0x9E37_79B9_7F4A_7C15u64;
        let mut noise_byte = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state as u8
        };

        let mut refused = Vec::new();
        for n in [
            1usize, 4, 14, 15, 16, 17, 24, 30, 31, 32, 33, 40, 47, 48, 49, 56, 63, 64, 65, 96, 127,
            128, 129, 269, 270, 512, 1000, 4096, 65_536,
        ] {
            let data: Vec<u8> = (0..n).map(|_| noise_byte()).collect();

            let mut block = Vec::new();
            block.push((n.min(15) as u8) << 4);
            if n >= 15 {
                let mut rest = n - 15;
                while rest >= 255 {
                    block.push(255);
                    rest -= 255;
                }
                block.push(rest as u8);
            }
            block.extend_from_slice(&data);

            let mut out = vec![0u8; n + keva_asm::unpack::UNPACK_SLACK];
            if !keva_asm::unpack::unpack_into_slice(
                &block,
                &mut out,
                n,
                keva_asm::unpack::Split::Even,
            ) {
                refused.push(n);
                continue;
            }
            assert_eq!(&out[..n], &data[..], "wrong bytes for a {n}-byte run");

            // And through every LZ4 body, which is where the wide copy for the
            // last run lives. Building the block by hand rather than asking
            // liblz4 for one is what lets this run everywhere: the interop test
            // needs the library linked, and there is no x86-64 build of it on
            // an Apple machine, so without this the changed path would be
            // exercised on rented hardware and nowhere else.
            for &body in keva_asm::unpack::Lz4Body::all() {
                let mut out = vec![0u8; n + keva_asm::unpack::UNPACK_SLACK];
                if !keva_asm::unpack::unpack_lz4_into_slice_on(body, &block, &mut out, n) {
                    refused.push(n);
                    continue;
                }
                assert_eq!(
                    &out[..n],
                    &data[..],
                    "the {body:?} LZ4 body produced wrong bytes for a {n}-byte run"
                );
                // Nothing past the declared length may have been written
                // outside the slack the caller promised.
                assert_eq!(
                    out.len(),
                    n + keva_asm::unpack::UNPACK_SLACK,
                    "the buffer moved"
                );
            }
        }

        assert!(
            refused.is_empty(),
            "the kernel refused an all-literal block at these lengths: {refused:?}"
        );
    }

    /// The *kernel* must read what liblz4 wrote, not merely the store.
    ///
    /// [`our_decoder_reads_what_liblz4_wrote`] goes through `unpack_into`,
    /// which falls back to the portable decoder the moment the kernel declines.
    /// So it passes whether the kernel reads an LZ4 block or refuses every one
    /// of them, and it has been passing while the kernel refused.
    ///
    /// That is the failure mode this crate has already paid for once: a kernel
    /// that merely declines leaves every output-comparing test green, and the
    /// only thing that noticed was a benchmark assertion on rented hardware.
    /// This asks the kernel directly, one shape at a time, and says which ones
    /// it refused.
    #[cfg(feature = "liblz4")]
    #[test]
    fn the_kernel_itself_reads_what_liblz4_wrote() {
        if !keva_asm::unpack::asm_available() {
            return;
        }

        #[link(name = "lz4")]
        extern "C" {
            fn LZ4_compress_default(src: *const u8, dst: *mut u8, n: i32, cap: i32) -> i32;
        }

        let mut state = 0x9E37_79B9_7F4A_7C15u64;
        let mut noise_byte = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state as u8
        };

        let mut cases: Vec<(String, Vec<u8>)> = Vec::new();
        for size in [16usize, 64, 512, 1000, 4096, 65_536] {
            cases.push((format!("records_{size}"), records(size)));
            cases.push((format!("runs_{size}"), vec![b'a'; size]));
            cases.push((
                format!("noise_{size}"),
                (0..size).map(|_| noise_byte()).collect(),
            ));
        }

        let mut refused = Vec::new();
        for (label, data) in &cases {
            let mut block = vec![0u8; data.len() + 1024];
            let n = unsafe {
                LZ4_compress_default(
                    data.as_ptr(),
                    block.as_mut_ptr(),
                    data.len() as i32,
                    block.len() as i32,
                )
            };
            assert!(n > 0, "liblz4 refused {label}");
            block.truncate(n as usize);

            // Every body, not just this machine's. Dispatch runs one of them
            // here, so a divergence in any other would sit unseen until it
            // reached a part nobody tests on -- which has happened once
            // already, to the Xeon decoder on Emerald Rapids.
            let bodies = keva_asm::unpack::Lz4Body::all();
            assert_eq!(
                bodies.len(),
                if cfg!(target_arch = "x86_64") { 4 } else { 1 },
                "a body was added or removed without this test noticing"
            );
            for &body in bodies {
                let mut out = vec![0u8; data.len() + keva_asm::unpack::UNPACK_SLACK];
                let ok = keva_asm::unpack::unpack_lz4_into_slice_on(
                    body,
                    &block,
                    &mut out,
                    data.len(),
                );
                if !ok {
                    refused.push(format!(
                        "{label} on {body:?} ({} block bytes)",
                        block.len()
                    ));
                    continue;
                }
                assert_eq!(
                    &out[..data.len()],
                    &data[..],
                    "the {body:?} LZ4 body decoded {label} to the wrong bytes"
                );
            }
        }

        assert!(
            refused.is_empty(),
            "the assembly decoder refused {} of {} liblz4 blocks: {}",
            refused.len(),
            cases.len(),
            refused.join(", ")
        );
    }

    /// Adopting this format needs no flag day, and that is the point of
    /// reading LZ4 at all.
    ///
    /// A store whose values are LZ4 blocks can move to this one value at a
    /// time: the old bytes still decode, and whatever gets rewritten comes back
    /// in the new format. No offline conversion, no second decoder linked
    /// forever to read the past, no moment where both must be true at once.
    ///
    /// Two things the caller still owes, neither of them ours. The uncompressed
    /// length, because the LZ4 *block* format has never carried it -- anyone
    /// storing blocks already keeps it. And a bit saying which format an entry
    /// is in, because the two are not distinguishable by inspection: ours opens
    /// with a length varint and an LZ4 block opens with a token, and no byte
    /// value separates them.
    #[cfg(feature = "liblz4")]
    #[test]
    fn an_lz4_store_migrates_one_value_at_a_time() {
        #[link(name = "lz4")]
        extern "C" {
            fn LZ4_compress_default(src: *const u8, dst: *mut u8, n: i32, cap: i32) -> i32;
        }

        let mut cases: Vec<Vec<u8>> = Vec::new();
        for size in [512usize, 1024, 4096, 65_536] {
            cases.push(records(size));
        }

        let (mut was, mut now, mut migrated) = (0usize, 0usize, 0usize);
        let mut value = Vec::new();
        let mut repacked = Vec::new();
        let mut check = Vec::new();

        for original in &cases {
            // What is already on disk: an LZ4 block and a length beside it.
            let mut legacy = vec![0u8; original.len() + 1024];
            let n = unsafe {
                LZ4_compress_default(
                    original.as_ptr(),
                    legacy.as_mut_ptr(),
                    original.len() as i32,
                    legacy.len() as i32,
                )
            };
            assert!(n > 0, "liblz4 refused {} bytes", original.len());
            legacy.truncate(n as usize);

            // A read: the old entry decodes with no LZ4 library present.
            unpack_into(&legacy, &mut value, original.len())
                .unwrap_or_else(|e| panic!("legacy entry of {} bytes: {e}", original.len()));
            assert_eq!(&value[..], &original[..], "the old value came back wrong");

            // A write: it goes back in the new format.
            assert!(
                pack(&value, &mut repacked),
                "{} bytes did not pack",
                value.len()
            );
            unpack(&repacked, &mut check).expect("what we just wrote");
            assert_eq!(
                &check[..],
                &original[..],
                "the migrated value came back wrong"
            );

            was += legacy.len();
            now += repacked.len();
            migrated += 1;
        }

        eprintln!(
            "  migrated {migrated} values: {was} bytes of LZ4 -> {now} of ours, {:.1}% smaller",
            100.0 * (1.0 - now as f64 / was as f64)
        );
        assert!(now < was, "the rewrite must not cost space: {was} -> {now}");
    }

    /// The other direction, and it does not hold: liblz4 refuses what we wrote.
    ///
    /// The even split is the LZ4 token layout, so the body of such a value
    /// looks like an LZ4 block. It is not one. The format also constrains where
    /// a block may end -- a literal run of at least five bytes to close, and no
    /// match inside the last twelve -- and the packer enforces neither. It runs
    /// matches to the final byte and then writes a zero token, which LZ4 has no
    /// concept of.
    ///
    /// The assertion is inverted on purpose: it pins what is true today rather
    /// than leaving the question open. Should the packer ever be taught LZ4's
    /// end rules, this fails, and the fix is to turn it into the positive test
    /// it is already shaped like.
    #[cfg(feature = "liblz4")]
    #[test]
    fn liblz4_refuses_what_we_wrote() {
        #[link(name = "lz4")]
        extern "C" {
            fn LZ4_decompress_safe(src: *const u8, dst: *mut u8, n: i32, cap: i32) -> i32;
        }

        let mut state = 0x9E37_79B9_7F4A_7C15u64;
        let mut noise_byte = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state as u8
        };

        let mut cases: Vec<Vec<u8>> = Vec::new();
        for size in [16usize, 17, 63, 64, 65, 1000, 4096, 65_536] {
            cases.push(records(size));
            cases.push(vec![b'a'; size]);
            cases.push((0..size).map(|_| noise_byte()).collect());
        }
        cases.push(b"ab".repeat(3000));
        cases.push(b"abcdefg".repeat(900));

        let mut packed = Vec::new();
        let (mut even, mut wide, mut refused) = (0, 0, 0);
        for data in &cases {
            if !pack(data, &mut packed) {
                continue;
            }
            let (raw, header) = get_varint(&packed).expect("a header");
            if Split::from_header(raw) != EVEN {
                wide += 1;
                continue;
            }
            even += 1;

            let body = &packed[header..];
            let mut out = vec![0u8; data.len()];
            let n = unsafe {
                LZ4_decompress_safe(
                    body.as_ptr(),
                    out.as_mut_ptr(),
                    body.len() as i32,
                    out.len() as i32,
                )
            };
            if n != data.len() as i32 {
                refused += 1;
                continue;
            }
            assert_eq!(
                &out[..],
                &data[..],
                "liblz4 read {} bytes wrong",
                data.len()
            );
        }
        eprintln!("  even {even}, wide {wide}, refused by liblz4 {refused}");
        assert!(
            even > 0,
            "no case chose the even split, so nothing was tested"
        );
        assert_eq!(
            refused, even,
            "liblz4 accepted one of our blocks -- see the note above"
        );
    }

    /// The probe judges the whole value, not the part of it it happens to read.
    ///
    /// Both of these are values a store really sees -- a buffer written at a
    /// high offset, a record of fixed size only partly filled, a ciphertext
    /// followed by plaintext -- and both were stored whole when the probe read
    /// only the head and the middle. The cost of that is not a few percent, it
    /// is the entire ratio: `mix_tail` packs to 41276 bytes of 65536 and was
    /// being kept at 65536.
    #[test]
    fn a_value_that_compresses_anywhere_is_not_written_off() {
        let noise = |n: usize| {
            let mut st = 0x2545_F491_4F6C_DD1Du64;
            (0..n)
                .map(|_| {
                    st ^= st << 13;
                    st ^= st >> 7;
                    st ^= st << 17;
                    st as u8
                })
                .collect::<Vec<u8>>()
        };

        // Compressible only in the last third, past the midpoint the probe
        // used to stop at.
        let mut tail = noise(40 * 1024);
        tail.resize(64 * 1024, 0);

        // Compressible only in the middle, missing both a head window and a
        // window at the halfway mark.
        let mut middle = noise(64 * 1024);
        middle[1024..31_000].fill(0);

        for (what, data) in [("tail", tail), ("middle", middle)] {
            let mut out = Vec::new();
            assert!(pack(&data, &mut out), "{what}: refused outright");
            assert!(
                out.len() * 3 < data.len() * 2,
                "{what}: {} of {} bytes, so the compressible part was never searched",
                out.len(),
                data.len()
            );
        }
    }

    /// What the probe is allowed to get wrong, and what it is not.
    ///
    /// [`worth_packing`] refuses a value on the strength of two kilobyte-wide
    /// windows, and refusing wrongly is expensive in a way that being slow is
    /// not: the value is stored whole, so the cost is the entire compression
    /// ratio, not a few percent of pack throughput. A random header in front of
    /// a body that repeats is a real shape -- a nonce, a checksum block, an
    /// embedded thumbnail -- and judging the value on its head alone would lose
    /// it. The second window is what this holds in place.
    #[test]
    fn a_random_head_does_not_write_off_the_body() {
        let mut st = 0x2545_F491_4F6C_DD1Du64;
        let mut input: Vec<u8> = (0..4096)
            .map(|_| {
                st ^= st << 13;
                st ^= st >> 7;
                st ^= st << 17;
                st as u8
            })
            .collect();
        while input.len() < 64 * 1024 {
            input.extend_from_slice(b"the body of this value repeats itself, ");
        }

        let mut out = Vec::new();
        assert!(pack(&input, &mut out), "refused a value that compresses");
        assert!(
            out.len() * 4 < input.len(),
            "{} of {} bytes: the body was not searched",
            out.len(),
            input.len()
        );
    }

    /// The kernel must *accept* what the packer writes, not merely agree with
    /// the portable decoder about the answer.
    ///
    /// Those are different claims, and the difference hid a register-clobbering
    /// bug for three commits. A kernel that declines returns zero, the caller
    /// runs the portable decoder, and the bytes come out right -- so every test
    /// that compares output passes while the assembly is never exercised at
    /// all. This one fails instead.
    #[test]
    fn the_kernel_accepts_every_stream_the_packer_writes() {
        if !keva_asm::unpack::asm_available() {
            return;
        }
        let mut packed = Vec::new();
        let mut out = Vec::new();
        let mut sizes: Vec<usize> = vec![256, 512, 1000, 1024, 4096, 16_384, 65_536];
        sizes.extend([5usize, 17, 64, 100]);
        for size in sizes {
            for input in [records(size), vec![b'a'; size]] {
                if !pack(&input, &mut packed) {
                    continue;
                }
                let f = frame(&packed).expect("the packer wrote a header");
                let took = match f.switch {
                    None => {
                        keva_asm::unpack::unpack_asm(f.body, &mut out, f.declared, f.split.kernel())
                    }
                    Some(switch) => keva_asm::unpack::unpack_asm_hybrid(
                        f.body,
                        &mut out,
                        f.declared,
                        switch,
                        f.split.kernel(),
                        f.split.other().kernel(),
                    ),
                };
                assert!(
                    took,
                    "the kernel declined {} bytes ({})",
                    input.len(),
                    if f.switch.is_some() {
                        "two sections"
                    } else {
                        "one section"
                    }
                );
                assert_eq!(&out[..], &input[..], "and then lost bytes");
            }
        }
    }

    #[test]
    fn the_assembly_decoder_agrees_with_the_portable_one() {
        if !keva_asm::unpack::asm_available() {
            return;
        }

        let mut state = 0x9E37_79B9_7F4A_7C15u64;
        let mut noise_byte = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state as u8
        };

        let mut cases: Vec<Vec<u8>> = Vec::new();
        for size in [
            1usize, 2, 3, 4, 5, 7, 8, 15, 16, 17, 31, 32, 33, 47, 48, 49, 64, 1000, 4096, 8192,
            65_536,
        ] {
            cases.push(records(size));
            cases.push(vec![b'a'; size]);
            cases.push((0..size).map(|_| noise_byte()).collect());
        }
        cases.push(b"ab".repeat(3000));
        cases.push(b"abc".repeat(2000));
        cases.push(b"abcdefg".repeat(900));
        cases.push([b"xy".repeat(500), vec![0u8; 4000], b"xy".repeat(500)].concat());

        // A *short* match at a short distance, which the cases above never
        // produce: `"ab".repeat(3000)` is one enormous match and takes the
        // extended path. This is the fast path's overlapping copy, where the
        // source is closer than one copy block and the bytes have to be laid
        // down offset by offset. It was unreachable from this corpus, which was
        // found by splicing a jump into that branch and watching every test
        // still pass.
        for period in [4usize, 5, 7, 8, 11, 12, 14, 16, 24, 31] {
            let mut v = Vec::new();
            let mut n = 0u8;
            while v.len() < 4096 {
                let chunk: Vec<u8> = (0..period)
                    .map(|_| {
                        n = n.wrapping_add(37);
                        n
                    })
                    .collect();
                // Twice, so the second copy is a match of exactly `period`
                // bytes at a distance of exactly `period`.
                v.extend_from_slice(&chunk);
                v.extend_from_slice(&chunk);
            }
            cases.push(v);
        }

        let mut packed = Vec::new();
        let mut asm_out = Vec::new();
        let mut ref_out = Vec::new();

        for input in &cases {
            if !pack(input, &mut packed) {
                continue;
            }
            let f = frame(&packed).expect("the packer wrote a header");
            let took = match f.switch {
                None => {
                    keva_asm::unpack::unpack_asm(f.body, &mut asm_out, f.declared, f.split.kernel())
                }
                Some(switch) => keva_asm::unpack::unpack_asm_hybrid(
                    f.body,
                    &mut asm_out,
                    f.declared,
                    switch,
                    f.split.kernel(),
                    f.split.other().kernel(),
                ),
            };
            assert!(
                took,
                "the kernel declined a stream the packer produced, {} bytes ({})",
                input.len(),
                if f.switch.is_some() {
                    "two sections"
                } else {
                    "one section"
                }
            );
            unpack_portable(&packed, &mut ref_out).expect("the portable decoder accepts it");

            assert_eq!(
                asm_out,
                ref_out,
                "decoders disagree on {} bytes",
                input.len()
            );
            assert_eq!(
                &asm_out[..],
                &input[..],
                "round trip lost {} bytes",
                input.len()
            );
        }
    }

    /// A corrupt stream may be refused by the kernel and named by the portable
    /// decoder, and that asymmetry is deliberate. What must never happen is the
    /// kernel accepting something the portable decoder would reject, or the two
    /// accepting it and producing different bytes — that is how a silently
    /// wrong value reaches a client.
    #[test]
    fn the_assembly_decoder_never_accepts_more_than_the_portable_one() {
        if !keva_asm::unpack::asm_available() {
            return;
        }

        let mut packed = Vec::new();
        assert!(pack(&records(4096), &mut packed));

        let mut broken: Vec<Vec<u8>> = Vec::new();
        for cut in [0usize, 1, 2, 3, 5, 9, 17, 33, 64, 128] {
            if cut < packed.len() {
                broken.push(packed[..packed.len() - cut].to_vec());
            }
        }
        for at in [0usize, 1, 2, 3, 4, 8, 16, 32, 64, 100] {
            for xor in [0x01u8, 0x0F, 0xF0, 0xFF] {
                if at < packed.len() {
                    let mut c = packed.clone();
                    c[at] ^= xor;
                    broken.push(c);
                }
            }
        }

        let mut asm_out = Vec::new();
        let mut ref_out = Vec::new();
        for case in &broken {
            let Ok(f) = frame(case) else {
                continue;
            };
            let took = match f.switch {
                None => {
                    keva_asm::unpack::unpack_asm(f.body, &mut asm_out, f.declared, f.split.kernel())
                }
                Some(switch) => keva_asm::unpack::unpack_asm_hybrid(
                    f.body,
                    &mut asm_out,
                    f.declared,
                    switch,
                    f.split.kernel(),
                    f.split.other().kernel(),
                ),
            };
            if !took {
                continue; // refused, which the portable decoder is free to name
            }
            let reference = unpack_portable(case, &mut ref_out);
            assert!(
                reference.is_ok(),
                "the kernel accepted a stream the portable decoder rejects"
            );
            assert_eq!(asm_out, ref_out, "decoders disagree on a corrupt stream");
        }
    }

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
        // Data that keeps the even split and has matches worth a second look.
        // Without it every case here either fails to pack or takes the wide
        // split and is skipped below, so the diff had nothing to compare on the
        // one path the kernel actually implements -- and stayed green through a
        // change to the search that moved 4 KiB of it by 94 bytes.
        for size in [1000usize, 4096, 65_536] {
            let mut mixed = Vec::with_capacity(size);
            let mut i = 0u64;
            while mixed.len() < size {
                mixed.extend_from_slice(
                    format!(
                        "{{\"k\":\"{:08x}\",\"role\":\"member\",\"n\":{}}}",
                        i.wrapping_mul(2654435761),
                        i % 97
                    )
                    .as_bytes(),
                );
                i += 1;
            }
            mixed.truncate(size);
            cases.push(mixed);
        }

        let mut rust_out = Vec::new();
        let mut asm_out = Vec::new();
        let mut round_trip = Vec::new();

        // Which portable packer this kernel is the counterpart of. The two are
        // not the same: AArch64 implements the eager, fixed-split search and
        // must be diffed against the reference that matches it, while the
        // x86-64 kernel implements the production pass and is diffed against
        // the packer that actually runs. Diffing either against the other's
        // reference would compare two different algorithms and could only pass
        // by accident.
        let production = keva_asm::pack_find::kernel_is_production_packer();

        // Coverage asserted rather than assumed: if the dispatch ever stops
        // reporting the bodies that exist, this test would quietly shrink to
        // one line and keep passing.
        #[cfg(target_arch = "x86_64")]
        assert_eq!(
            keva_asm::pack_find::PartLine::all().len(),
            3,
            "x86-64 has three assembled bodies and the test must drive all of them"
        );

        // Every assembled body, not merely the one this machine dispatches to.
        // The three part lines are identical today and this is what says so;
        // without it an Intel runner would never execute the AMD body, and a
        // divergence in it would wait for production to find it.
        for &line in keva_asm::pack_find::PartLine::all() {
            // Both sides start from the sentinel, not from zero: a zero slot reads
            // as position -1 and would send the kernel's verify load out of bounds.
            // Fresh per line, because the table is carried across records and a
            // line that inherited another's table would be searching a different
            // one.
            let mut rust_table = Box::new([EMPTY; HASH_SIZE]);
            let mut asm_table = keva_asm::pack_find::new_table();

            for input in &cases {
                let rust_kept = if production {
                    pack_with(input, &mut rust_out, &mut rust_table)
                } else {
                    pack_with_eager(input, &mut rust_out, &mut rust_table)
                };
                let asm_kept =
                    keva_asm::pack_find::pack_asm_on(line, input, &mut asm_out, &mut asm_table)
                        .is_some();

                // The eager kernel writes the even split only. Where the portable
                // packer chose the wide match it also made a second pass over the
                // table, so the two tables no longer hold the same thing -- both are
                // reset rather than letting the divergence follow into later cases.
                // The production kernel writes every split, so it has nothing to
                // skip.
                if rust_kept && !production {
                    let (raw, _) = get_varint(&rust_out).expect("a header");
                    if Split::from_header(raw) != EVEN {
                        rust_table.fill(EMPTY);
                        asm_table.fill(EMPTY);
                        continue;
                    }
                }

                assert_eq!(
                    rust_kept,
                    asm_kept,
                    "{line:?} disagreed on whether packing helped for {} bytes",
                    input.len()
                );

                if rust_kept {
                    assert_eq!(
                        rust_out,
                        asm_out,
                        "{line:?} produced a different encoding for the same {} byte input",
                        input.len()
                    );
                    unpack(&asm_out, &mut round_trip).expect("assembly output must unpack");
                    assert_eq!(&round_trip, input, "assembly output lost bytes");
                }
            }
        }
    }

    /// The kernel is assembled four times -- two splits by two table widths --
    /// and a diff that only ever reaches one of them is a diff that proves
    /// nothing about the other three.
    ///
    /// So this walks a randomised corpus and *counts* which bodies it entered,
    /// failing if any stayed cold. Coverage asserted rather than assumed: the
    /// eager corpus above reached the wide split on none of its cases for a
    /// while, and the test stayed green through a change that moved 4 KiB of
    /// output by 94 bytes.
    #[test]
    fn every_assembled_body_is_reached_and_agrees() {
        if !keva_asm::pack_find::asm_available()
            || !keva_asm::pack_find::kernel_is_production_packer()
        {
            return;
        }

        let mut state = 0x9E37_79B9_7F4A_7C15u64;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };

        // Shapes chosen to straddle the two thresholds the kernel branches on:
        // NARROW_TABLE_ABOVE picks the table width, and how often the match
        // field overflows decides whether the split changes partway.
        let mut cases: Vec<Vec<u8>> = Vec::new();
        for &size in &[512usize, 2049, 4096, 8192, 8193, 20_000, 65_536] {
            // Long matches, which overflow a four-bit match field and are what
            // drives the stream into the wide split.
            let mut runs = Vec::with_capacity(size);
            while runs.len() < size {
                let n = 20 + (next() % 200) as usize;
                let b = (next() % 7) as u8 + b'a';
                runs.extend(std::iter::repeat(b).take(n));
            }
            runs.truncate(size);
            cases.push(runs);

            // Short matches and long literal runs, which keep the even split.
            let mut mixed = Vec::with_capacity(size);
            let mut i = 0u64;
            while mixed.len() < size {
                mixed.extend_from_slice(
                    format!("{{\"id\":{},\"v\":\"{:x}\"}}", i % 32, next() & 0xFFFF).as_bytes(),
                );
                i += 1;
            }
            mixed.truncate(size);
            cases.push(mixed);

            // A literal run past 255, which is the chain the extended length
            // field encodes, followed by something that matches.
            let mut long_lit: Vec<u8> = (0..size.min(600)).map(|_| next() as u8).collect();
            long_lit.extend_from_slice(&long_lit.clone());
            cases.push(long_lit);
        }

        let mut seen_even = 0usize;
        let mut seen_hybrid = 0usize;
        let mut seen_narrow = 0usize;
        let mut seen_wide_table = 0usize;

        let mut rust_out = Vec::new();
        let mut asm_out = Vec::new();
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

            if !rust_kept {
                continue;
            }
            assert_eq!(
                rust_out,
                asm_out,
                "different encodings for the same {} byte input",
                input.len()
            );
            unpack(&asm_out, &mut round_trip).expect("assembly output must unpack");
            assert_eq!(&round_trip, input, "assembly output lost bytes");

            let (raw, _) = get_varint(&asm_out).expect("a header");
            if raw & HYBRID_BIT == HYBRID_BIT {
                seen_hybrid += 1;
            } else if Split::from_header(raw) == EVEN {
                seen_even += 1;
            }
            if input.len() > NARROW_TABLE_ABOVE {
                seen_narrow += 1;
            } else {
                seen_wide_table += 1;
            }
        }

        assert!(seen_even > 0, "no case stayed in the even split");
        assert!(
            seen_hybrid > 0,
            "no case changed split, so the wide bodies and the trailer went untested"
        );
        assert!(seen_wide_table > 0, "no case used the twelve-bit table");
        assert!(seen_narrow > 0, "no case used the eleven-bit table");
    }

    #[test]
    fn an_offset_before_the_start_is_refused() {
        // Declared length 8, one literal, then a match reaching back further
        // than anything produced.
        let mut stream = Vec::new();
        put_varint_into(8 << 2, &mut stream); // declared 8, even split, one section
        stream.push(1 << 4); // one literal, and a match-length nibble of zero
        stream.push(b'a');
        stream.extend_from_slice(&99u16.to_le_bytes()); // offset far too large

        let mut out = Vec::new();
        assert_eq!(unpack(&stream, &mut out), Err(PackError::BadOffset));
    }
}
