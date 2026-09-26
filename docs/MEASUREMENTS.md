# The measurement log

Append-only. One entry per question asked of a machine, newest at the bottom.

This file exists because of a specific failure. `docs/ROADMAP.md` carried
"records_4k: -22.1% size" for the repeat-offset code for four commits. It was an
upper bound computed from the offset distribution, with the price of the token
bit never subtracted, and it was written in a tone that made it read as a
result. Four kernels were adapted on the strength of it. The real number is
-3.6%, and the decoder lost 22.9% on records_512.

The roadmap says what we intend. This file says what a machine actually
answered, including the answers that killed the idea that asked the question.

## What an entry owes the reader

- **the revision**, so it can be rerun
- **the machine**, because a number from an M2 Max is not a number from an EPYC
- **the method** -- how many runs, how long, against which baseline
- **every cell**, not the flattering ones
- **what it refuted**, if anything. An entry that only confirms an expectation
  is worth a third of one that removes a possibility.

Throughput is GiB/s against the *unpacked* bytes, always, so a smaller output at
the same wall clock reads as a higher number. Sizes are bytes.

## Method, unless an entry says otherwise

Criterion, `--measurement-time 0.5 --warm-up-time 0.3`, one run, filtered to the
cells the question is about, compared against a saved baseline of the same shape
and the same measurement time. Roughly three seconds per question on the M2 Max.

The long form -- three runs of every cell at two seconds -- takes twenty minutes
and answers "by exactly how much". The short form answers "did it move, and
which way", which is what a question in flight needs. Short numbers may only be
compared against short numbers: at 0.5 s the cache state differs enough that the
absolute GiB/s does not match the long form.

Rounds worth remembering about the noise floor: liblz4 and lz4_flex sit in the
same runs and touch none of our code, so their movement is the instrument's
error bar. In the long run below it was +/-1.5%. Anything smaller than that is
not a finding.

---

## 2026-09-18 -- the repeat-offset bit, 3/4/1, full table

M2 Max, medians of three, 2 s per cell, 162 cells. Baseline `12760b2` (before
the format changed) against `56770f8` (all four kernels adapted).

The even token went from four bits of literal length and four of match to
three, four, and one bit meaning "the offset the block before wrote", with the
two offset bytes then not written at all.

### Our own format, decoding -- GiB/s

| shape | before | after | | liblz4 before | after | | lz4_flex before | after | |
|---|---|---|---|---|---|---|---|---|---|
| records_512 | 12.37 | 9.81 | **-20.7%** | 10.98 | 11.06 | +0.7% | 10.64 | 10.71 | +0.6% |
| varied_512 | 10.85 | 8.47 | **-22.0%** | 8.54 | 8.64 | +1.2% | 8.90 | 9.03 | +1.5% |
| records_4k | 14.71 | 13.91 | -5.5% | 12.45 | 12.37 | -0.7% | 10.91 | 10.99 | +0.7% |
| varied_4k | 8.10 | 7.62 | -5.9% | 6.78 | 6.82 | +0.5% | 7.00 | 7.02 | +0.3% |
| records_64k | 15.27 | 15.42 | **+1.0%** | 11.24 | 11.20 | -0.4% | 11.60 | 11.63 | +0.3% |
| varied_64k | 8.12 | 7.40 | -9.0% | 4.98 | 4.99 | +0.3% | 7.26 | 7.67 | +5.8% |
| noise_4k | 64.21 | 71.29 | +11.0% | 64.44 | 65.16 | +1.1% | 39.80 | 39.99 | +0.5% |
| noise_512 | 54.83 | 55.65 | +1.5% | 45.36 | 47.12 | +3.9% | 34.32 | 34.46 | +0.4% |
| noise_64k | 57.41 | 57.42 | +0.0% | 55.86 | 58.21 | +4.2% | 37.26 | 38.89 | +4.4% |

### Standard LZ4, decoding -- GiB/s

Unchanged within the error bar on every cell, which is the point: the LZ4 bodies
were not touched, so this table is the instrument measuring itself.

| shape | before | after | | liblz4 before | after | | lz4_flex before | after | |
|---|---|---|---|---|---|---|---|---|---|
| records_512 | 11.60 | 11.57 | -0.2% | 11.06 | 11.00 | -0.6% | 9.84 | 9.93 | +0.8% |
| varied_512 | 8.22 | 8.20 | -0.3% | 8.51 | 8.64 | +1.5% | 7.78 | 7.83 | +0.6% |
| records_4k | 12.01 | 12.23 | +1.9% | 12.50 | 12.50 | +0.0% | 10.79 | 10.84 | +0.5% |
| varied_4k | 6.52 | 6.55 | +0.3% | 6.85 | 6.88 | +0.4% | 6.78 | 6.81 | +0.4% |
| records_64k | 11.13 | 11.20 | +0.7% | 11.21 | 11.32 | +1.0% | 11.84 | 11.79 | -0.4% |
| varied_64k | 5.53 | 5.51 | -0.3% | 4.92 | 5.01 | +1.8% | 5.09 | 5.12 | +0.8% |
| noise_4k | 64.44 | 65.00 | +0.9% | 64.82 | 64.83 | +0.0% | 39.72 | 40.14 | +1.1% |
| noise_512 | 47.44 | 47.00 | -0.9% | 46.57 | 46.56 | -0.0% | 33.82 | 34.45 | +1.9% |
| noise_64k | 39.29 | 39.03 | -0.7% | 57.01 | 57.98 | +1.7% | 39.32 | 38.00 | -3.4% |

### Packing -- GiB/s

| shape | before | after | | liblz4 before | after | | lz4_flex before | after | |
|---|---|---|---|---|---|---|---|---|---|
| records_512 | 2.41 | 2.44 | +1.1% | 1.19 | 1.09 | -8.3% | 1.51 | 1.50 | -0.6% |
| varied_512 | 1.31 | 1.30 | -1.0% | 0.89 | 0.85 | -4.2% | 0.99 | 0.99 | +0.0% |
| records_4k | 3.70 | 3.72 | +0.6% | 2.68 | 2.60 | -3.0% | 2.97 | 2.96 | -0.3% |
| varied_4k | 1.24 | 1.23 | -0.8% | 1.27 | 1.27 | -0.4% | 1.17 | 1.17 | -0.4% |
| records_64k | 3.54 | 3.66 | +3.3% | 3.04 | 3.11 | +2.2% | 3.49 | 3.50 | +0.1% |
| varied_64k | 0.97 | 0.95 | -2.6% | 0.92 | 0.97 | +5.9% | 1.16 | 1.17 | +1.0% |
| noise_4k | -- | -- | -- | 5.82 | 5.82 | -0.1% | 5.54 | 5.59 | +0.9% |
| noise_512 | 2.76 | 2.90 | +5.2% | 1.34 | 1.36 | +1.4% | 1.58 | 1.62 | +2.6% |
| noise_64k | 149.94 | 148.13 | -1.2% | 22.24 | 22.01 | -1.0% | 18.19 | 18.29 | +0.5% |

### Size

| shape | raw | before | after | | liblz4 |
|---|---|---|---|---|---|
| records_512 | 512 | 195 | **180** | -7.7% | 192 |
| varied_512 | 512 | 356 | 340 | -4.5% | -- |
| records_4k | 4096 | 603 | 581 | -3.6% | 660 |
| varied_4k | 4096 | 2229 | 2222 | -0.3% | 2329 |
| records_64k | 65536 | 6509 | 6487 | -0.3% | 7722 |
| varied_64k | 65536 | 33180 | 33801 | **+1.9%** | 33005 |

**What this refuted.** The roadmap's -22.1% on records_4k. The real figure is
-3.6%, and varied_64k grows.

**What it is worth anyway.** records_512 crosses liblz4 in the other direction:
195 B was 1.6% *larger* than liblz4's 192 B, 180 B is 6.3% smaller. That is the
first cell where our own format wins on size rather than trails.

---

## 2026-09-18 -- where the 20% went

Short form, M2 Max, baseline `12760b2` at the same measurement time.
Reproduced the long run's finding in three seconds: -22.9%, -21.5%, -6.9%,
-9.1% on records_512, varied_512, records_4k, varied_64k.

### Refuted first: it is not misprediction

Made the offset load unconditional and selected with `csel` instead of
branching on the repeat bit.

| shape | vs. the branching version |
|---|---|
| records_512 | +0.3% |
| varied_512 | **-8.6%** |
| records_4k | -0.2% |
| varied_64k | **-18.8%** |

Worse, and worst where the repeat rate is lowest (varied_64k, 5.6%) -- there the
branch predicts almost perfectly and `csel` puts the load back on the dependency
chain for nothing. The branch stays.

The equal loss at unequal predictability had already said this before the
experiment: records_512 repeats 90% of the time and varied_512 65%, one near the
best case for a predictor and one near the worst, and both lost 21%.

### The block census

Same corpus, same packer, walked block by block under each layout.

| shape | blocks 4/4 -> 3/4/1 | literal chain 4/4 -> 3/4/1 | match chain | repeat | decode |
|---|---|---|---|---|---|
| records_512 | 12 -> 12 | 16.7% -> **41.7%** | 50.0% -> 50.0% | 75.0% | -22.9% |
| varied_512 | 18 -> 18 | 33.3% -> **66.7%** | 5.6% -> 5.6% | 61.1% | -21.5% |
| records_4k | 53 -> 93 | 17.0% -> 17.2% | 22.6% -> 11.8% | 45.2% | -6.9% |
| varied_4k | 186 -> 186 | 26.9% -> **60.8%** | 18.8% -> 18.8% | 18.8% | -5.9% |
| records_64k | 903 -> 1791 | 4.9% -> **2.3%** | 11.1% -> 4.7% | 43.0% | **+1.0%** |
| varied_64k | 3054 -> 3054 | 24.2% -> **55.7%** | 19.7% -> 19.7% | 5.6% | -9.1% |

(The records_4k and records_64k block counts are not comparable: those values
switch split partway and the census walks one layout throughout.)

**The finding.** Cutting the literal field from four bits to three roughly
doubles the share of blocks whose literal length needs an extension chain, and
records_64k -- the one cell whose rate *fell* -- is the one cell that gained.

**Why that field and not the other.** It is written in `unpack.S`: a saturated
match length is handled inside the loop, "a saturated nibble is not a reason to
leave". A saturated literal length branches out to `L(fast_litlong)` and back.

---

## 2026-09-18 -- the same bit taken from the match field instead, 4/3/1

Four bits of literal, three of match, one repeat. Same baseline, same method.

| shape | 3/4/1 | 4/3/1 |
|---|---|---|
| records_512 | -22.9% | **-5.5%** |
| varied_512 | -21.5% | **-33.7%** |
| records_4k | -6.9% | **-2.7%** |

Census under 4/3/1:

| shape | blocks | literal chain | match chain |
|---|---|---|---|
| records_512 | 12 | 16.7% | **75.0%** |
| varied_512 | 18 | 33.3% | **44.4%** |
| varied_4k | **309** (was 186) | 2.6% | 5.2% |
| varied_64k | **5032** (was 3054) | 0.5% | 3.0% |

**What this refuted.** That a match chain is cheap because it stays in the loop.
It does stay, but it splits long matches into more blocks -- varied_4k goes from
186 blocks to 309 for the same value -- and block count is what the decoder pays
per byte.

**What both entries together say.** On real data both length ceilings are
already tight. A bit taken from either field costs more than the offset it
saves; which field it hurts depends on the shape. records wants its literal
field, varied wants its match field. So the bit cannot come out of a length
field at all -- it needs a layout of its own, chosen per value, or a place
outside the token.

Reverted. `56770f8` (3/4/1) stands.

---

## 2026-09-18 -- three streams instead of one

M2 Max, portable throwaway (`examples/streams.rs`, deleted after this entry).
One matcher, two serialisations of its identical block list, two decoders
written in the same style with the same wide copies, so the only variable is the
layout.

**The hypothesis.** The decoder is latency-bound, not issue-bound -- `unpack.S`
measured that and says so in as many words. The chain per block is

    load token -> extract literal length -> form offset address -> load offset

two dependent loads with arithmetic between them. Put the offsets in a stream of
their own and the address becomes `offsets + 2*i`, which depends on nothing, so
both loads issue together. Zstd is built this way.

**Method note, which is half the entry.** The first attempt took the minimum of
seven short bursts. It reported records_512 anywhere between 3.14 and 8.17 GiB/s
across three runs of the same binary -- a factor of 2.6 on a number meant to
resolve 5% -- and produced a +50.7% and a +57.3% that were pure artefact. Small
inputs finish inside one scheduling quantum, so a burst either lands on a clean
slice or it does not, and the minimum keeps whichever the turbo state favoured.
Replaced with fixed 20 ms spans, the two layouts alternating within each round so
drift hits both, medians of nine rounds.

### Result -- three runs, each the median of nine alternating rounds

| shape | run 1 | run 2 | run 3 | size |
|---|---|---|---|---|
| records_512 | +0.0% | -2.0% | -2.0% | +1.5% |
| varied_512 | -2.1% | +0.0% | +0.1% | +0.8% |
| records_4k | -0.2% | +0.0% | +0.7% | +0.6% |
| varied_4k | +2.8% | +0.3% | -0.2% | +0.2% |
| records_64k | +0.0% | -0.0% | +1.1% | +0.0% |
| varied_64k | -2.6% | -0.6% | +5.1% | +0.0% |

Absolute throughput 4.4 to 7.6 GiB/s, against 8 to 15 for the kernels -- close
enough that the same bottleneck should be visible, far enough that this is
evidence and not proof.

**What this refuted, and it is the idea the roadmap called the larger structural
one.** Splitting the streams buys nothing. Zero on every shape, and the size
goes very slightly the wrong way because the header grows two varints.

**Why, and this is the part worth keeping.** There are two chains per block, not
one, and only the cheap one was addressed. The *address* chain -- token to
literal length to offset address -- is what three streams break. The *progress*
chain is `produced += lit; produced += mat`, and the position of the next token
depends on it in both layouts. An out-of-order core overlaps the address chain
across blocks quite happily; it cannot overlap the progress chain, because it
does not know where the next block starts until this one's lengths are decoded.

That is also why "fewer and bigger blocks" worked twice at about 19% each. It
is the only lever that touches the progress chain: it runs it fewer times.

---

## 2026-09-18 -- two interleaved chains

M2 Max, portable throwaway (`examples/interleave.rs`, deleted after this entry).
Same method as the streams entry: fixed spans, alternating, medians of nine,
three runs.

**The hypothesis.** Throughput is a monotone function of bytes per block --

| shape | blocks | bytes/block | GiB/s |
|---|---|---|---|
| varied_64k | 3054 | 21.5 | 8.12 |
| varied_4k | 186 | 22.0 | 8.10 |
| varied_512 | 18 | 28.4 | 10.85 |
| records_512 | 12 | 42.7 | 12.37 |
| records_64k | 903 | 72.6 | 15.27 |
| records_4k | 53 | 77.3 | 14.71 |

-- monotone across two data shapes and three orders of magnitude, so a block
costs about the same whatever its size. If that cost is the progress chain
(`produced += lit; produced += mat`, which decides where the next token is) then
it is latency and cannot be overlapped across blocks. Two independent chains
could be overlapped. Split the value in half, give each half its own cursors and
matches that never reach across the boundary, and step them alternately.

### Result

| shape | run 1 | run 2 | run 3 | size |
|---|---|---|---|---|
| records_512 | -11.9% | -10.6% | -11.5% | **+57.3%** |
| varied_512 | +2.4% | +2.7% | +2.6% | +20.8% |
| records_4k | +1.2% | +0.8% | +1.7% | +25.7% |
| varied_4k | +3.1% | +2.6% | +2.2% | +5.1% |
| records_64k | -3.4% | -4.1% | -3.3% | +4.4% |
| varied_64k | -0.7% | +1.0% | -3.2% | +1.4% |

Block counts barely moved (records_512 14 -> 13, varied_4k 243 -> 225), so the
halves did not cost the matcher much. The size did: the second half starts with
no history, and on a 512-byte value that is 57% more output.

**What this refuted, and it is my reading of the bottleneck rather than only
this idea.** Two independent chains in flight buy nothing. If the per-block cost
were latency, this had to nearly halve it.

**So the cost is work, not waiting.** "Not issue-bound at the margin" is what
the earlier counter run showed, and I read that as latency-bound. It is not the
same thing, and `unpack.S` already said which: *the copy loops are port-bound at
five micro-ops per 32 bytes*. Two chains share those ports. So does a third
stream. That is why both of today's structural ideas measured zero, and it is
consistent with the one thing that did move -- the repeat bit removed a load per
block and gained 1.0% on records_64k, the shape where nothing else got worse.

**What is left, then.** Fewer micro-ops per output byte, by either route:

- fewer blocks, which is packer work and the lever that paid twice at ~19%
- fewer bytes read per block, which is format work: one-byte offsets where the
  value's offsets all fit (100% of them on records_512, 31% on records_4k), the
  repeat bit where it pays. Both of these are per-*value* decisions, so they
  belong in the header where they cost the token no bits and the decoder no
  cycles -- only a choice of which kernel runs.

---

## 2026-09-18 -- one-byte offsets, decided by a census rather than a benchmark

Ten seconds, no benchmark needed: the question was how many bytes a header bit
saying "every offset in this value is one byte" would save, and that is a count.

| shape | packed B | offsets written | all < 256 | saves |
|---|---|---|---|---|
| records_512 | 180 | **2** | yes | 2 B = 1.1% (9 blocks repeat) |
| varied_512 | 340 | 7 | no | 0 |
| varied_4k | 2222 | 150 | no | 0 |
| varied_64k | 33801 | 2882 | no | 0 |

**What this refuted.** Its own premise. The earlier offset census -- 100% of
records_512's offsets below 256, 31% of records_4k's -- was taken *before* the
repeat bit existed. The repeat bit removes the same bytes, and it got there
first: records_512 writes eleven offsets under 4/4 and two under 3/4/1. One byte
each would now save two bytes.

And on varied the header form never fires: "all of them fit" is false on every
varied shape. A bit *per block* would fire, but that is the thing measured twice
today as costing more than it saves.

**Still open.** records_4k and records_64k switch split partway and the census
walks one layout, so they are not in the table. They are also where the old
statistics were most interesting -- offsets were 48% of records_64k's output.
Worth a census that follows the switch.

---

## 2026-09-18 -- the repeat bit moved to the wide split

M2 Max. Even goes back to four and four; wide becomes two, five and one. All
four kernels, Rust and both architectures. Baseline measured in the same run as
the result, which matters -- see the method note below.

### Size, and this part is a count

| shape | before | after | | liblz4 | |
|---|---|---|---|---|---|
| records_4k | 603 B | **488 B** | -19.1% | 660 B | 26% smaller |
| records_64k | 6509 B | **5524 B** | -15.1% | 7722 B | 28% smaller |
| records_512 | 195 B | 195 B | -- | 192 B | |
| varied_512 / 4k / 64k | unchanged | | | | never reach wide |

Block counts do not move: records_4k stays at 96 blocks, records_64k at 1567.
The bytes come out of offsets alone -- 58 of records_4k's 96 blocks and 493 of
records_64k's 1567 now write none.

### Throughput

| shape | vs. before |
|---|---|
| records_4k | **-0.3%** |
| records_64k | **-3.5%** |
| records_512 | -0.2% |
| varied_512 | -0.4% |
| varied_64k | -0.6% |

records_4k is 19% smaller for free. records_64k costs 3.5%, which is outside
the +/-1.5% noise floor and is the one open item.

### Method note, and it cost an hour

The first measurement of this change read **-7.1% on records_4k** against a
baseline saved three hours earlier at the same settings. Re-baselining inside
the same run turned that into -0.3%. The machine drifts; a saved Criterion
baseline is only comparable to a run taken near it.

An hour went into chasing the -7.1% through three hypotheses, all refuted and
all worth recording because they are the obvious suspects:

- **the branch on the repeat bit** -- reversing it so the repeat case falls
  through changes nothing, because both arms already take exactly one taken
  branch. `tbnz` taken for a repeat, or `tbnz` not taken and `b` taken for a
  load: one either way.
- **code placement** -- `.p2align 6` on the loop head instead of `.p2align 4`:
  -7.8%, slightly worse.
- **the narrower fixed match move** -- MAT_CAP falls from 67 to 35 with the
  five-bit field, taking COPY_MAX from 96 to 64. Forcing MAT_CAP back to 67:
  -6.9%, unchanged.

None of them was the cause because there was no cause. The number was an
artefact of the baseline.

---

## 2026-09-18 -- the header: what it actually carries

Not a benchmark. Three counts and two facts read out of the code, because the
header was about to be redesigned on assumptions again.

### Prior art: the prefix varint exists and the tradeoff is known

The scheme -- length in the leading bits of the first byte instead of a
continuation bit per byte -- is **PrefixVarint**. 7-Zip ships it, WebAssembly
weighed it against LEB128 and kept LEB128 (design#601), and `prefix_uvarint` is
a Rust crate. Both claims we argued out are settled in the literature:

> *"PrefixVarint is expected to be faster to encode and decode and **has the
> same encoded size as LEB128**."*

> *"This improves coding speed by **reducing the number of branches** evaluated
> to code longer values."*

`h14s.p5r.org/2024/11/11/bigger-better-varints.html` compares the family and
finds no scheme that beats LEB128 on size below 35 bits. That is not an
implementation gap: the length costs one bit per byte wherever it is written.
Our maximum is 29 bits (`MAX_UNPACKED` = 512 MiB), so we are entirely inside the
range where every scheme is the same size. **Any header change is a decode-speed
change, not a size change.**

### Header bytes by scheme, counted

`declared` is the *unpacked* length; today's encoding is `varint(declared << 2 |
hybrid << 1 | split)`.

| declared | today LEB<<2 | 2-bit prefix + x3 | LEB + x3 | truncated unary + x3 |
|---|---|---|---|---|
| 32 - 42 | 2 | 2 | **1** | **1** |
| 48 - 4095 | 2 | 2 | 2 | 2 |
| 4096 - 5460 | 3 | **2** | **2** | **2** |
| 8192 - 65536 | 3 | 3 | 3 | 3 |
| 1 MiB | 4 | **3** | 4 | 4 |
| 256 MiB | 5 | **4** | 5 | out of range |

One-byte ceiling: today **31**, a fixed 2-bit prefix **21**, LEB or unary with
base-3 flags **42**. A fixed two-bit prefix costs two bits in the one-byte case
where a unary prefix costs one; it wins back more than that above a megabyte.

### Two facts that make the small case free

**A value of 63 bytes or less can only be plain even, never hybrid, never wide.**

1. The switch is gated on `seen >= SPLIT_WINDOW` with `SPLIT_WINDOW = 16`, and a
   block produces at least `MIN_MATCH` = 4 bytes. So a value needs **at least 64
   bytes of output** before the packer even evaluates the switch.
2. Nothing starts in the wide split. All six `pack_pass` call sites pass `EVEN`
   (pack.rs:385, 394, 769, 773, 847, 849), and `unpack_into_slice` writes
   `EVEN.bit()` too. The wide split is reachable *only* through the hybrid
   switch.

So for values up to 63 bytes the flag state is a constant, and a prefix that
means "short and plain" needs no flag bits at all. Six bits of payload stay six
bits of payload.

### And bit 0 of the header is dead

It follows from fact 2: `split` is never set on write, so `Split::from_header`
reads a bit that is always zero. Every header dumped today was flags=0 or
flags=2, never 1 or 3. The header carries **one** bit of real information and
`<< 2` pays for two, halving the range that fits in a byte for nothing.

**What this sets up.** The header has three live states, not four, and only one
of them can occur below 64 bytes. `MAX_UNPACKED` at 512 MiB needs 29 bits, which
does not fit a 2-bit prefix's 30-bit top class once flags are folded in by
multiplication -- so either the top class carries an extra byte, or the flags sit
in fixed bits below the prefix, or the limit drops to 256 MiB. That is the one
open decision.

---

## 2026-09-18 -- the prefix header, built

Replaced `varint(declared << 2 | hybrid << 1 | split)` with

```text
  00 LLLLLL              1 byte,   6 bits, up to 63
  01 H LLLLL + 1 byte    2 bytes, 13 bits, up to 8191
  10 H LLLLL + 2 bytes   3 bytes, 21 bits, up to 2097151
  11 H LLLLL + 3 bytes   4 bytes, 29 bits, up to 536870911
```

One flag, not two, because `split` was dead. The short class carries no flag at
all because it cannot need one, and keeps six bits of payload. Decoding is one
four-byte load, a table-indexed mask and a shift -- no loop, no branch on the
length. Rust, both assembly packers and the bench all read and write it; 99
tests green on aarch64 and 99 on x86-64.

### What it did to size

| shape | packed | liblz4 | lz4_flex |
|---|---|---|---|
| records_4k | **487 B** | 658 B | 660 B |
| records_64k | **5524 B** | 7992 B | 7722 B |
| varied_4k | **2228 B** | 2333 B | 2329 B |
| varied_512 | **356 B** | 366 B | 368 B |
| varied_64k | 33180 B | **32730 B** | 33005 B |

One byte on records_4k (488 -> 487), as counted in advance. Everything at 64
bytes or more is in the two-byte class either way, so that is the whole size
story -- as the prior-art entry said it would be.

### The one case that moved, and it is not the header

`records(32)` packs to **31 bytes with a one-byte header**. Under LEB128 the
header was two bytes, the output came to 32, and `out.len() < input.len()` was
false -- so the value was **stored raw**. The shorter header pushed it over the
line.

That is not obviously a win:

| | throughput |
|---|---|
| stored raw (LEB128 header, packer refused) | 6.31 GiB/s |
| packed (prefix header, one byte saved) | 1.75 GiB/s |

Reading it went from a `memcpy` to a decode. **-72% to save one byte of 32.**

**What this refutes: the "worth packing" rule.** `out.len() < input.len()` says
yes to a value that saves a single byte and costs three quarters of its read
speed. The rule needs a margin, and the header change is what made that visible
-- it is not a reason to keep the longer header.

### What is not measured

Decode throughput of the header itself at 192 bytes and up. The bench parses the
header too, so it cannot run against both encodings, and a stashed-source A/B
crashes on the mismatch. At 512 bytes the header is 2 bytes of 195 and the
decode is a handful of cycles out of ~130, so the expectation is "inside the
noise floor" -- but that is an expectation, not a number, and it is written here
as one.

---

## 2026-09-18 -- what a token split costs varied, from the tool that was already here

`examples/blocks.rs` has been in the repo since `e9aadfe` and contains
`split_cost()`, which walks a packed stream and reports, for every split from
2/6 to 6/2, the share of blocks that would need an extension chain. It answers
in ten seconds the question this morning was spent guessing at twice.

### varied_64k

| split | ext-L | ext-M | blocks needing a chain |
|---|---|---|---|
| 2/6 | 74.6% | 0.0% | 74.6% |
| 3/5 | 55.7% | 3.6% | 58.2% |
| **4/4** | 24.2% | 19.7% | **43.9%** |
| 5/3 | 0.1% | 66.6% | 66.7% |
| 6/2 | 0.0% | 97.2% | 97.2% |

### varied_4k

| split | ext-L | ext-M | blocks needing a chain |
|---|---|---|---|
| 2/6 | 77.8% | 0.0% | 77.8% |
| 3/5 | 60.5% | 1.6% | 61.1% |
| **4/4** | 27.0% | 18.9% | **45.9%** |
| 5/3 | 0.5% | 61.1% | 61.6% |
| 6/2 | 0.5% | 98.9% | 98.9% |

**4/4 is the minimum on both.** The curve is a clean U -- the two lengths trade
against each other and the even split sits at the bottom. varied_4k at 3/5 is
61.1% against 44.9% at 4/4, which is the regression measured this morning as
-5.9%, available beforehand as a count.

### Block counts against the competition

| varied_64k | blocks | B/block | size | GiB/s |
|---|---|---|---|---|
| **keva** | **3054** | **21.5** | 33180 B | **8.12** |
| liblz4 | 4872 | 13.5 | 32730 B | 4.98 |
| lz4_flex | 3298 | 19.9 | 33005 B | 7.26 |

liblz4 spends 60% more blocks on the same bytes and reads them at 61% of our
speed. The bytes-per-block relationship holds across implementations, not only
across our own revisions -- which is the strongest evidence for it so far.

**Conclusion: varied is finished.** The layout is provably at its optimum, the
block count already leads both competitors, and buying fewer blocks by searching
harder was measured at -24% pack throughput for -2.7% blocks. 21.5 bytes per
block is a property of the data.

---

## 2026-09-18 -- three rented machines that measured nothing

Intel (c3-standard-4), AMD (c3d-standard-4) and ARM (Axion), `QUICK=1`,
`12760b2` against `06b0d92`. Every run completed, every machine was deleted on
the way out, and the throughput numbers are worthless.

Each run measured exactly two cells:

```
pack/sizes/pack/2048      1.30 GiB/s
pack/sizes/unpack/2048    6.73 GiB/s
```

**Why.** `bench-on-gce.sh` held the Criterion filter in a variable called
`GROUPS`. Bash defines `GROUPS` itself -- it holds the caller's group ids -- and
assigning to it is silently ignored. So the filter reached the remote runner as
the string `20`, Criterion matched that against benchmark ids, and `2048` was
the only thing in the suite that contained it.

```
$ bash -c 'echo $GROUPS; GROUPS="compress3|own_format"; echo $GROUPS'
20
20
```

Renamed to `KEVA_FILTER`. The bug was silent in both directions: the script
reported success, the machines reported success, the tests passed (103 green on
each), and the result files are full of plausible-looking Criterion output.

**What the runs did confirm**, because the bench prints sizes while setting up,
and they are identical on all three architectures and to the M2 Max:

```
records_4k:   keva  487 B   liblz4  658 B   lz4_flex  660 B
records_64k:  keva 5524 B   liblz4 7992 B   lz4_flex 7722 B
varied_4k:    keva 2228 B   liblz4 2333 B   lz4_flex 2329 B
varied_512:   keva  356 B   liblz4  366 B   lz4_flex  368 B
```

**And one thing worth keeping about cost.** A cold c3-standard-4 went from
`create` to measuring in minutes, not the twenty the script's own comments
claim. There is no case for leaving machines up between runs, and idle machines
are billed. `KEEP=1` stays off.

---

## 2026-09-18 -- all four parts, the whole day's format work

`12760b2` (before anything today) against `6652ba2` (wide repeat bit + prefix
header). `QUICK=1`, `KEVA_FILTER=own_format`, one run per revision, one second
per cell. liblz4 and lz4_flex sit in the same runs and touch none of our code,
so their movement is each machine's error bar.

### Our own format, decoding -- percent change

| shape | M2 Max | Axion | Xeon | EPYC |
|---|---|---|---|---|
| records_512 | -0.2% | +1.2% | +0.1% | +0.1% |
| varied_512 | -0.4% | -0.3% | -0.2% | +1.2% |
| records_4k | -0.3% | **+10.8%** | -12.1% | -4.9% |
| varied_4k | -- | -0.6% | +3.8% | +4.0% |
| records_64k | -3.5% | **+13.7%** | -1.4% | -2.1% |
| varied_64k | -0.6% | +0.7% | -0.2% | -0.6% |
| noise_512 | -- | -2.0% | -50.3% | +1.5% |
| noise_4k | -- | +1.4% | +36.0% | +0.2% |
| noise_64k | -- | +0.2% | -1.2% | -9.4% |

### The error bars, from code we did not touch

| | liblz4 worst | lz4_flex worst |
|---|---|---|
| Axion | +1.4% | -4.1% |
| EPYC | +8.5% | **-19.4%** |
| Xeon | **-12.6%** | +2.5% |

**What can be read from this, and what cannot.**

- **Axion is the clean measurement and it is a large win.** Controls within
  +/-1.4%, and the two cells the wide repeat bit touches gain 10.8% and 13.7%.
  Those are exactly the two shapes that read 15-19% fewer bytes. A core with a
  narrower memory path converts the size saving into throughput; the M2 Max,
  with bandwidth to spare, does not.
- **Xeon is unreadable.** liblz4 moves 12.6% on varied_512 with no code change,
  and the two extreme keva cells are `noise`, which is stored raw -- a memcpy
  that no format change can touch. Single run, one second a cell, shared VM.
- **EPYC is marginal.** Controls reach 19.4%; records_4k at -4.9% is inside
  that. Also the A-B-B-A did not survive the file naming: the script names
  result files by sha, so positions 3 and 4 overwrote 1 and 2. What is compared
  above is position 4 against position 3, which is *less* biased than 1-vs-2 but
  is not the design.
- **The M2 Max is quiet on everything**, the -3.5% on records_64k included; it
  is the one machine where three separate runs agreed.

**The honest summary.** One machine of four shows a double-digit gain, one shows
a small loss, two are too noisy to say. The size result is not in question and
does not depend on the machine: records_4k 603 -> 487 bytes, records_64k 6509 ->
5524, both around 30% under liblz4. Anyone wanting a throughput number to quote
needs medians of three, and that is not what QUICK is for.

---

## 2026-09-18 -- a 48-byte fixed match move

`f6490c8` (64-byte move) against `64f5537` (48). Axion, `QUICK=1`, one run.

The wide split carries 35 bytes of match inline since the repeat bit took a
match bit. `COPY_MAX` followed `MAT_CAP` from 96 down to 64 on its own, but its
steps were 32/64/96 and 35 wants 48 -- so the fixed move wrote a fourth sixteen
bytes for nothing, on the body 1552 of records_64k's 1567 blocks pass through.

| shape | 64 B | 48 B | | liblz4 | lz4_flex |
|---|---|---|---|---|---|
| records_4k | 12.38 | **12.76** | **+3.1%** | -0.1% | -0.1% |
| records_64k | 12.39 | **12.81** | **+3.4%** | +0.7% | +0.6% |
| records_512 | 10.94 | 11.36 | +3.8% | -3.6% | +2.1% |
| varied_512 | 8.70 | 8.76 | +0.7% | -0.6% | -0.4% |
| varied_4k | 7.32 | 7.33 | +0.1% | -0.4% | +1.5% |
| varied_64k | 7.01 | 7.02 | +0.2% | -0.0% | -0.1% |
| noise_4k | 77.49 | 77.39 | -0.1% | +0.6% | -0.1% |
| noise_64k | 85.70 | 86.34 | +0.7% | -1.9% | -1.0% |

**The mechanism is confirmed by the rows that did not move.** `varied` decodes
under the even split, where `MAT_CAP` is 19 and `COPY_MAX` was and remains 32 --
+0.1%. `noise` is stored raw and never decoded -- +0.7%. Only `records` reaches
the wide body, and only `records` gains. That is exactly the shape of the
prediction, which is stronger evidence than the size of the number.

Controls sit under 1% on records_4k and records_64k, so +3.1% and +3.4% are
real. records_512 at +3.8% is inside its own control's 3.6% and is not a claim.

M2 Max on the same change: **-0.6%**, its noise floor -- the same non-answer it
gave the repeat bit, and for the same reason. Two changes now, both aimed at
reading or writing fewer bytes, both invisible on a machine with bandwidth to
spare and both worth 3-14% on one without.

---

## 2026-09-18 -- packed or raw, swept from L1 to RAM

M2 Max, three runs. N values of 512 bytes each in two arenas -- one raw, one
packed -- read in a fixed random order, one at a time, both sides writing into a
buffer that already has its capacity. Throughput against unpacked bytes.

| values | raw MiB | packed MiB | run 1 | run 2 | run 3 |
|---|---|---|---|---|---|
| 64 | 0.03 | 0.01 | -77.8% | -76.8% | -77.8% |
| 1 024 | 0.5 | 0.2 | -72.5% | -72.2% | -73.1% |
| 8 192 | 4.0 | 1.5 | -58.1% | -68.1% | -71.2% |
| **65 536** | **32.0** | **12.3** | **+4.2%** | **+13.5%** | **+4.7%** |
| 262 144 | 128.0 | 48.8 | -44.1% | -46.7% | -46.0% |
| 1 048 576 | 512.0 | 194.9 | -54.5% | -55.0% | -53.8% |

**Three regimes, three different bottlenecks.**

*In cache, to about 4 MiB.* Raw runs at 44-61 GiB/s, the decoder at 12-13.
Arithmetic against memcpy, a factor of four to five. This is the worst case for
packing and the only one an earlier measurement here looked at.

*The window at 32 MiB raw.* The raw arena falls out of the M2 Max's system level
cache; the packed one at 12.3 MiB still fits. Raw drops from 21 to 8.6 GiB/s,
packed holds at 9, and **packed wins** -- in all three runs.

*Past that, 128 MiB and up.* Both arenas are in RAM, both are miss-bound, and
the decode is charged on top. Packing loses 45-55%. Worth noting that the packed
side reads 62% fewer bytes and is still slower: at 1.57 GiB/s the limit is cache
misses and decode, not bandwidth.

**What this does not measure, and it is the point of packing.** 512 MiB of RAM
holds 2.6x as many values packed. That is not a throughput effect; it is the
difference between a working set fitting and not fitting, and the sweep above
shows what happens at exactly that boundary.

**For the "worth packing" rule.** The cost is a factor of four to five on reads
at any size, so the question is only what the space buys. records(32) saves one
byte in thirty-two -- 3% -- for that factor. A 512-byte value saves 62%. The
rule `out.len() < input.len()` cannot tell those apart and should be a ratio.

---

## 2026-09-18 -- OUT_MARGIN was already right, and the worth-packing rule was not

**OUT_MARGIN.** After COPY_MAX fell to 48 the margin dropped to 16 -- the bound
`LIT_MAX + COPY_MAX - 64` went negative, leaving only the heuristic constant. It
looked worth re-tuning. It is not: M2 Max, against 16 as the baseline,

| | records_512 | varied_512 | records_4k |
|---|---|---|---|
| 8 | -0.67% | -0.01% | -0.60% |
| 24 | -0.44% | +0.49% | -0.95% |
| 32 | -0.17% | -0.88% | -1.09% |

all inside +/-1%. The constant answers "how long is a final literal run", which
the format change did not touch. Left at 16.

**The worth-packing rule.** `out.len() < input.len()` replaced with
`worth_storing`: at least an eighth saved, and at least eight bytes. The read
costs a factor of four to five whatever the size, so the space is the only thing
bought and it has to clear a bar.

| original | packed | before | after |
|---|---|---|---|
| 32 B | 31 B (3%) | stored packed | **stored raw** |
| 192 B | 134 B (30%) | packed | packed |
| 512 B | 195 B (62%) | packed | packed |
| 4096 B | 487 B (88%) | packed | packed |

Nothing the benchmark packs is affected -- every shape saves between 30% and
88%. The one value that changes behaviour is the one that was trading 78% of its
read speed for 3% of its size.

---

## 2026-09-18 -- the whole day, all four parts, properly

`12760b2` against `bbac13d`: wide repeat bit, prefix header, 48-byte match move,
worth-storing rule. M2 Max is medians of three runs at 2 s a cell; the three
rented parts are single runs at 3 s a cell. Controls in the same runs.

### records cells, decoding our own format

| | Axion | M2 Max | Xeon | EPYC |
|---|---|---|---|---|
| records_4k | **+12.9%** | **-7.2%** | **-14.4%** | -5.2% |
| records_64k | **+18.8%** | **-4.8%** | +2.2% | -1.1% |
| worst control | 1.0% | 0.5% | 3.8% | 11.0% |

### everything else

`varied` and `noise` move within their controls on every part: +/-0.3% on
Axion, +/-0.2% on the M2 Max, +/-1.4% on Xeon, +/-5.1% on EPYC. Neither reaches
the wide split, so neither should move, and neither does.

### Size, which does not depend on the machine

| shape | before | after | | liblz4 |
|---|---|---|---|---|
| records_4k | 603 B | **487 B** | -19.2% | 658 B |
| records_64k | 6509 B | **5524 B** | -15.1% | 7992 B |
| varied_4k | 2229 B | 2228 B | -- | 2333 B |
| varied_512 | 356 B | 356 B | -- | 366 B |

**The finding, and it reverses what single runs said.** Three parts of four lose
5-14% on records_4k. Only Axion gains, and it gains a lot. The M2 Max number is
the most trustworthy figure in the table -- medians of three with controls under
0.5% -- and it is **-7.2%**, where an earlier single run reported -0.3%.

**A retraction.** Two hours ago this file would have said x86 loses because the
repeat offset lives in the stack frame there while AArch64 keeps it in x27. The
M2 Max is AArch64, uses x27, and loses 7.2%. The split is not architectural.
Axion is the outlier, not x86, and what makes it one is not established.

**Where that leaves the day's work.** Size is unambiguous and machine
independent: about 30% under liblz4 on records shapes, 15-19% under our own
previous format. Throughput was traded for it on three parts of four. Whether
that trade is right is a decision about the store, not about the decoder -- the
sweep from L1 to RAM in the entry above shows the size paying for itself once
the working set stops fitting, and none of these cells measure that.

---

## 2026-09-19 -- the input margin, and where the day landed

`12760b2` (before any of this) against `581683c` (wide repeat bit, prefix
header, 48-byte match move, worth-storing rule, and both margin fixes).
`QUICK=1`, 3 s a cell, one run per revision on the rented parts; the M2 Max
figures are medians of three from the local runs.

### What the margin fix did, per part

records_4k, before and after the fix:

| | before | after |
|---|---|---|
| Axion | +12.9% | **+18.5%** |
| EPYC | -5.2% | **+7.7%** |
| Xeon | **-14.4%** | **+3.7%** |
| M2 Max | -7.5% | ~-2.3% |

The fix is one constant. `L(entry)` stopped the fast loop a fixed distance
short of the input's end, and past it every block runs on the checked path at
three to four times the price. That distance was sized for the even split --
seventeen bytes on AArch64, **thirty-seven** on x86 -- and the wide split, which
carries three literals instead of fourteen, paid it too. Five is enough.

On x86 that was seven blocks of a records_4k value on the expensive path where
one belongs. It had been there since the wide split existed.

### Where that leaves the format against the competition

Our own format, decoding, on the three rented parts:

| | vs liblz4 | vs lz4_flex |
|---|---|---|
| Axion | +27.8% to **+58.1%** | +2.0% to +25.4% |
| Xeon | +24.8% to **+65.8%** | +9.7% to +41.0% |
| EPYC | +8.7% to +45.1% | +0.7% to +25.9% |

Eighteen cells, eighteen wins against both libraries. And smaller at the same
time: records_4k 487 B against liblz4's 658, records_64k 5524 against 7992 --
26% and 31% under.

### Reading foreign LZ4 blocks, which is the other half

| | vs liblz4 | vs lz4_flex |
|---|---|---|
| records + varied | **14 of 18** | 11 of 18 |
| noise | **1 of 9** | 6 of 9 |

78% ahead of liblz4 on compressible data, and a real gap on `noise`: -30.1%,
-35.8%, -37.1%, -32.6%. Those are all-literal blocks, where liblz4 has a path
ours does not match -- lz4_flex loses there too, so it is liblz4 being good
rather than us being broken. Unexplained and untouched.

### What the day actually taught

Eight ideas were tried. **Seven measured zero or worse**: three streams, two
interleaved chains, one-byte offsets, the repeat bit in the even split, and
three separate instruction-level optimisations of the loop head. The one that
worked came from profiling and then *counting* -- how many blocks of a 487-byte
value fall past a 17-byte margin at 5.1 bytes a block -- rather than from
reasoning about what ought to be faster.

Also worth keeping: a 0.5-second Criterion run lied four times today, twice by
inventing a gain and twice by hiding one. Medians of three at one second or
more, baseline measured in the same session, or the number does not count.

---

## 2026-09-20 -- profiling the LZ4 body, which nothing had done

Three attempts at the `noise` gap were aimed at the copy loop and measured
+5.5%/-0.7%, -3.6% and +1.6%. All three were reasoned from the source. The
profiling example only ever drove `pack::unpack`, so the interop kernel had
never been sampled. It takes `lz4_*` shapes now and feeds it a block liblz4
wrote.

### noise: the chain, not the copy

```
lz4_noise_64k   .Ll_lz4_lit_blk   83.1%    the copy, already the good branch
                .Ll_litlen_byte   15.6%    <- the length chain
lz4_noise_4k    .Ll_lz4_lit_blk   72.6%
                .Ll_litlen_byte   11.8%
```

A 64 KiB literal run under LZ4's token spends **257 bytes** in the chain, read
one at a time. The loop was eleven instructions a byte, one of which
rematerialised MAX_UNPACKED every round. Hoisted, closing on its own test: nine.

**same_bytes/noise_64k 39.85 -> 50.29 GiB/s, +26.2%**, against a control moving
-5.3%. Everything else within 0.7%.

### records and varied: two different bottlenecks

```
lz4_records_4k  .Ll_fast_near     47.1%   <- overlapping match copy
                .Ll_fast          12.9%
                .Ll_fast_lendone  10.9%
                .Ll_fast_offset    9.9%

lz4_varied_4k   .Ll_fast          33.9%
                .Ll_fast_offset   33.1%
                .Ll_fast_copy     11.1%
```

`fast_near` handles matches whose offset is shorter than their length. It does
not appear in any profile of our own format all day -- LZ4 carries eighteen
bytes of match against our wide split's thirty-five, so records-shaped data
produces far more short overlapping matches under its token.

And it is reached by a **call**: four `mov` to marshal, `bl`, `ret`, one `mov`
back -- six instructions of scaffolding around roughly ten of work, on a path
holding 47% of the decode. That is about 17% of the whole spent on the call
itself, and it is the shape the no-mixed-calls rule exists to prevent.

`L(repeat)` itself is well built: splat tables and `tbl` to lay down an
overlapping pattern in one go. Inlining its short case into `fast_near` is the
next step and is not done.

### M2 Max, everything, medians of three

Own format: **18 of 18** against both libraries, +0.6% to +79.4%.
Foreign LZ4 blocks: 10 of 18 -- 7 of 9 against lz4_flex, 3 of 9 against liblz4.
The gap is -1% to -4% on most cells and -20.7% on noise_64k.

Reading a format someone else designed against the implementation that designed
it is the harder half, and it is where the remaining work is.

## 2026-09-20 — memcpy in the LZ4 literal path: +34.4% on noise_64k

The question was "why don't we call memcpy in the noise case?" — and the answer
to yesterday's first attempt: we do, but in the wrong place. The call sat in
`L(slow_lit_wide)`, and a 64 KiB literal run is turned away there (`x16 + 32 >
x20`, because a run that reaches the end of the input has no 32 bytes left
behind it). It lands in `L(slow_lit_exact)` and from there in
`L(lz4_lit_blk)`. The call never ran.

The profile after the chain fix puts the time exactly there, 5076 samples:

    .Ll_lz4_lit_blk    4377   86.2%
    .Ll_litlen_byte     637   12.5%

### The threshold, measured rather than guessed

`same_bytes/keva/noise_*`, M2 Max, Criterion 2 s, medians of three, baseline
measured in the same run. Three thresholds one after another:

| LIT_MEMCPY |  512 B |  4 KiB | 64 KiB |
|------------|--------|--------|--------|
| 512        | −10.4% |  −0.2% | +26.7% |
| 4096       |  +0.3% |  −1.8% | +36.8% |
| **8192**   |  −0.1% |  +0.8% | **+34.4%** |

A call costs about what the loop gains on 512 bytes, and at 4096 the two are
even — but only if 4096 still falls *inside* the loop. With the threshold at
4096 a 4 KiB value takes the call by a hair and loses 1.8%; above that the row
is clean. Hence 8192 and not 4096.

In absolute terms, against the controls in the same run: noise_64k 42.5 -> 57.2
GiB/s, liblz4 57.4, lz4_flex 40.1. A deficit of 26.5% becomes parity.

### That our own format does not move is shown, not measured

`objdump -d` over `unpack.o`, symbol by symbol before and after the change: all
55 `.Le_*` (even) and all 47 `.Lw_*` (wide) are instruction-identical. What
changed is `.Ll_slow_lit_exact` (2 -> 8), and `.Ll_lz4_lit_loop` is new. A
throughput comparison would only have shown the machine's noise here — across
those same runs lz4_flex moved 9.3% on `varied_512`.

### Open

x86_64 already has `rep movsb` for this path behind `REP_MIN 1024`, but only on
the Xeon line; EPYC deliberately does not, because Zen has ERMSB without FSRM.
Whether a `memcpy` call does the same thing there as it does here is unmeasured.

## 2026-09-20 — The long match: four instructions instead of eight, but not everywhere

The profile after the memcpy commit shows all three losing foreign-format cells
sitting entirely in the fast loop -- no slow path left, and `rep_checked`,
which was once 47.1% on `lz4_records_4k`, does not appear at all any more:

    lz4_varied_4k     fast 33.4%  fast_offset 32.9%  fast_copy 10.4%
    lz4_records_64k   fast 24.8%  fast_lendone 23.2%  fast_offset 22.1%
                      fast_longout 8.9%  fast_longblk 8.2%
    lz4_records_4k    fast 25.8%  fast_lendone 21.1%  fast_offset 19.3%

`fast_longblk` and `fast_longout` together are 17.1% and 14.2%, and the loop
carried the same bookkeeping that was already thrown out of `litcp_block`: a
counter, a comparison, two pointer additions and an unconditional jump back
around two moves. Post-indexing and `subs` turn eight into four.

### Two changes, one measurement error

Both block loops were rebuilt first, the match and the literal variant. Medians
of three, baseline from the same session block:

| own format | both | match only |
|---|---|---|
| records_4k  | +5.2% | +5.6% |
| records_512 | +4.5% | +5.0% |
| records_64k | +5.0% | +4.0% |
| records_192 | **-3.7%** | +1.1% |

The literal half was the damage and has been reverted. Why it hurts is not
measured; that it hurts, three times over.

### And one loop in two forms

The match half on its own won our format and lost the foreign one:

    own_format/keva/records_64k   14.82 -> 15.47   +4.4%   controls +1.0%
    same_bytes/keva/records_64k   11.11 -> 10.76   -3.2%   controls +1.2%

So both forms are kept, with `#ifdef KEVA_LZ4` between them. The reason is
placement, not work: the addresses, the bytes and the iteration count are
identical, but the LZ4 body sends 99% of its records blocks through here and
ours a fifth, and sixteen bytes less code in front of a loop that hot is not
free. Two forms for a measured difference of 7.7%.

### Where it stands after, medians of three, net of the controls

| own format | delta | net |  | foreign LZ4 | delta | net |
|---|---|---|---|---|---|---|
| records_4k  | +6.8% | +6.5% | | records_4k  | +0.5% | +0.7% |
| records_512 | +5.3% | +6.6% | | records_512 | +0.1% | +0.6% |
| records_64k | +4.2% | +4.2% | | records_64k | +0.3% | +0.5% |
| noise_64k   | +0.9% | +3.2% | | noise_512   | -1.7% | -1.8% |

Own format: 22 of 22 cells. Foreign LZ4 unchanged on 10 of 18; the largest
deficit is now varied_4k at -4.4% against liblz4.

x86_64 has the same loop and is unmeasured.

## 2026-09-20 — varied in the foreign format: the census, and a refuted idea

The task was the flow of varied_4k and varied_512 in the foreign LZ4 format,
where we sit 4.4% and 2.6% behind liblz4. A census over the LZ4 block counts
what the decoder is asked per block -- and alongside it, how often the answer
changes from one block to the next, because that is what the predictor sees:

| | varied_512 | varied_4k | records_4k | records_64k |
|---|---|---|---|---|
| bytes per block | 18.9 | 17.5 | 36.9 | 32.5 |
| literals / match | 2.6 / 15.3 | 2.0 / 15.5 | 2.2 / 34.7 | 0.2 / 32.3 |
| match nibble saturated | 11.1% | 24.8% | 76.6% | 75.0% |
| of those, a change | 22.2% | 34.2% | 38.7% | 49.2% |
| match lengths 4-14 / 15-31 | 73% / 27% | 49% / 47% | 25% / 47% | 25% / 47% |
| literal lengths = 0 | 33% | 56% | 32% | 88% |

Two things records does not have. varied carries **17.5 bytes per block**, half
what records carries, and its match lengths sit **on the threshold**:
saturation begins at 19 bytes, and 49% fall below it, 47% above. The branch
`cmp w26,#15 / b.eq fast_long` changes direction on every third block and is
therefore not predictable.

### The arithmetic that explains the rest

The loop header says the loop runs at 8.7 cycles per block. 17.5 bytes over 8.7
cycles at 3.5 GHz is **6.5 GiB/s**, and varied_4k measures 6.48. So this shape
is bound entirely by the token chain and not by work -- liblz4 runs the same
blocks at roughly 8.3 cycles.

### And the idea that followed from it, which is wrong

The chain is `ldrb` (4) -> `lsr` (1) -> `add x8` (1) -> `add x23, #2` (1).
AArch64 can shift and add in one instruction, so `add x8, x7, x4, lsr
#LIT_SHIFT` instead of `lsr` + `add` -- on paper one cycle out of seven, a good
11%.

Measured, two runs, against the controls:

| | foreign LZ4 | own format |
|---|---|---|
| records_4k | -5.9% | -6.0% |
| records_64k | -5.6% | -6.5% |
| varied_4k | -4.1% | -5.2% |
| varied_64k | -9.7% | -6.1% |

The shifted add is not one cycle on M2 but two, and the `lsr` had to stay
regardless -- the comparison and the output cursor both want the number. So one
instruction more and not one cycle less. Reverted.

That makes the eighth idea to fail at this same spot, and all eight tried to
shorten the chain or save instructions. What has ever worked in this kernel was
something else: more bytes per block (wide split, lazy matching, about 19%
each) or less work altogether (memcpy, +34%). In the foreign format the block
size is not ours to decide.

## 2026-09-20 — The short saturated match: built, measured, nothing

The arithmetic said nothing and the measurement says nothing, but the
arithmetic was in instructions, and instructions are not the currency in this
loop -- so it was built anyway.

The nibble saturates at nineteen bytes; the fixed move writes thirty-two. Every
match in between went through a length byte, the guard chain and the block loop
in order to have written the same thirty-two bytes that `L(fast_copy)` does in
one store pair. On varied that is a fifth of all blocks. Two instructions added
(`cmp x14,#COPY_MAX` / `b.ls L(fast_copy)`), about five saved, only under
KEVA_LZ4 -- the wide split writes forty-eight, and the margin `L(fast_litlong)`
checks is thirty-two.

Medians of three against a baseline of six runs with unchanged code:

| foreign LZ4 | net | | own format | net |
|---|---|---|---|---|
| varied_4k | +0.6% | | records_4k | -0.4% |
| varied_512 | -0.8% | | records_64k | +0.4% |
| records_4k | -0.8% | | varied_512 | -3.2% |
| records_64k | -1.3% | | noise_512 | +3.1% |

Reverted. The ninth idea at this spot, and the first whose result had been
worked out beforehand -- five instructions on a fifth of the blocks is 0.5% of
the instructions, and a saved instruction buys no cycle here. That has stood at
the head of unpack.S since the `ccmp` attempt and still holds.

### What the run calibrated on the side

`own_format/varied_512` measures -3.2% even though the change sits behind
`#ifdef KEVA_LZ4` and our bodies never see it. So a median of three carries
about 3% of noise on this machine, and anything below that is not a
measurement. The controls in those same runs were within ±0.5%, which is why
the net column counts and the delta column does not.

## 2026-09-20 — The decoder executed its own tables

Not a measurement, a bug, and found while making a measurement safe.

The epilogue of the repeat routine sat after the `#endif` that closes the
shuffle masks:

```asm
L(rep_ret):
#ifdef KEVA_LZ4
    ... 512 bytes of masks ...
#endif
    ldp     x29, x30, [sp], #32
    ret
```

In every body without the tables, `L(rep_ret)` reaches the `ret` directly. In
the LZ4 body there are five hundred and twelve bytes of data in between, and
every return from the doubling ladder executed them as code.
`EXC_BAD_INSTRUCTION` in `.Ll_splat_first`, which is the middle of the first
mask.

Triggered by any foreign LZ4 block with a match reaching further back than it
is long, at an offset of sixteen or more -- below sixteen the shuffle path
catches it first. A 63-byte value with period 23 is enough:

    declared=63  block=38 bytes  ->  SIGILL

Why nobody saw it: the tables sit behind `#ifdef KEVA_LZ4`, and every test
decodes our own format. The benchmark shapes do not meet the condition --
`records`, `varied` and `noise` have no overlapping match at offset >= 16 in
liblz4's output. A hundred green tests and eighteen green benchmark cells, and
the decoder crashed on the first file that repeats itself differently.

The x86 body never had it: there, `L(splat_ret): ret` stands in front of the
tables. The aarch64 port lost the `ret` when it took the code over.

Two tests for it. `the_kernel_reads_a_match_that_overlaps_itself` builds blocks
by hand -- offsets 1 to 40, lengths up to 300 -- and runs them on every body
and without liblz4; without the fix it ends in SIGILL. And
`examples/lz4_soak.rs` puts 5760 liblz4-packed cases through the kernel and
compares byte for byte.

The lesson is not "more tests". It is that an `#ifdef` variant is a second
program, and that this second program hung here on nothing but a benchmark,
which measures throughput and not correctness.

## 2026-09-20 — The length byte for free, and the tenth refutation

The profile with finer labels, `varied_4k`, 6728 samples:

    .Ll_fast        22.1%   the two zone comparisons, so the jump back
    .Ll_px_guard    20.3%   the five instructions right after b.eq L(fast_long)
    .Ll_fast_offset 12.9%   ldrh of the offset, and the cursor
    .Ll_px_token    12.3%   load the token, shift, nibbles
    .Ll_fast_copy   11.1%   the fixed move

`px_guard` is five cheap ALU instructions, 12% of the instructions and 20% of
the time, and it sits at the landing point of the branch the census identified
as unpredictable at 34% direction changes.

`varied_512` shows 20.5% on the checked path alongside it (`slow_lit_ready`
8.0, `slow_mat_ready` 4.5, `slow_lit_done` 3.7, `tail` 2.9, `slow` 1.4) --
that is the seventeen-byte input margin on a value that breaks into about 27
blocks. Left alone.

### The idea

The byte that extends a saturated match sits two past the offset. A 32-bit load
at the same address carries both and costs what the 16-bit load it replaces
costs -- no second load, no second address, and that is exactly what
distinguishes it from the attempt documented in the source at -18%. The length
is then formed with `csel`, and a branch on "fits in the fixed move" replaces
the branch on "nibble saturated": 3% instead of 25%, and predictable.

### Measured twice, the same both times

The first time, `b.hi L(fast_lenbig)` fell into its own target, so every block
ran through the block loop -- varied -26%. Label taken out of the fallthrough
and measured again, baseline three runs, new code two:

| foreign LZ4 | net |
|---|---|
| varied_512 | **-30.8%** |
| varied_64k | -22.3% |
| varied_4k | -18.7% |
| records_64k | -9.8% |
| records_4k | -5.7% |
| noise_512 | +2.2% |

The reason is in the diff: `cinc x23, x23, eq`. The next block's input cursor
used to depend only on the literal count -- `ldrb` -> `lsr` -> `add x8` ->
`add x23, #2`. Now it also depends on the match nibble, on its comparison and
on the `cinc`: two cycles more on the one chain this loop is demonstrably bound
by. The mispredict saved is 3.4 cycles on a third of the blocks, a good cycle
on average; two cycles on every block costs more.

Our own format untouched, all eleven rows between -1.1% and +1.6% net -- the
change sits behind `#ifdef KEVA_LZ4`.

That makes the tenth idea to fail at this spot, and the third in a row to have
lengthened the chain without it being noticed while writing it. The rule is
sharp enough by now to apply in advance: **whatever computes `x23` for the next
block may not touch anything new.**

## 2026-09-20 — The padded remainder: the margin costs nothing now

The first idea since the memcpy that holds, and it comes out of the same
profile as the three before it. `lz4_varied_512` spent 20.5% on the checked
path:

    slow_lit_ready 8.0   slow_mat_ready 4.5   slow_lit_done 3.7
    tail 2.9   slow 1.4

The reason is the input margin. The fast path reads a fixed distance from the
cursor -- a token and the literal copy, seventeen bytes -- and stops that far
short of the end rather than checking per block. A 512-byte varied value packs
to about 250 bytes in 27 blocks, so seventeen bytes is nearly two of them, on a
path three to four times as expensive.

### What was built

When the *input* cursor leaves the zone -- and only then; the output margin is
a separate question -- the remaining seventeen bytes at most are copied into
sixty-four zeroed bytes in the frame, and `x19` is shifted by the cursor so
that every index in the loop keeps its meaning. Nothing else in the body knows.
The margin is set to the real end, and the fast path runs all the way there.

That padding cannot hide a truncated stream survives three arguments: any block
reading a byte from the end leaves the cursor past the end; an offset of zero
is refused where it is read; and `L(done)` checks that the cursor lands exactly
on the end, failing which the portable decoder takes over and produces the
error.

### The copy itself was only half the gain

Measured byte by byte: the checked path falls from 20.5% to 11.6%, but
`.Ll_pad_byte` shows up at **6.5%**. Seventeen bytes once per value is not
nothing on a 512-byte value. Replaced with a width ladder -- 16, 8, 4, 2, 1,
only the set bits cost anything -- so about fifteen instructions instead of a
hundred.

### Measured, medians of 3 paired rounds, net of the controls

| own format | net | | foreign LZ4 | net |
|---|---|---|---|---|
| records_192 | **+8.1%** | | varied_512 | **+3.3%** |
| noise_64k | +4.0% | | records_4k | +1.4% |
| records_4k | +2.6% | | varied_64k | +1.2% |
| varied_512 | +2.2% | | noise_512 | -2.7% |
| records_512 | +1.1% | | noise_64k | -3.3% |
| varied_4k | -0.7% | | records_64k | -0.4% |

`varied_512` in the foreign format now sits at -0.2% against liblz4 instead of
-3.1%.

The two `noise` rows are the only ones that resist, and the padding does not
fire there at all: a literal run covering the whole block leaves the fast path
at the first token, long before the margin matters. In those same runs liblz4
moved up by +2.6% and +2.7% on exactly those two cells, which is why the net
column is negative. Placement, not mechanics -- the kernel went from a 96-byte
frame to a 160-byte one.

x86_64 has the same margin and is unmeasured.

## 2026-09-20 — Four bytes where one was read

The question was whether the close cells in the foreign format were still
winnable. Counted in cycles per block -- the only unit in which shapes are
comparable -- the deficit was tiny:

| | B/block | lzvolt | liblz4 | flex | missing |
|---|---|---|---|---|---|
| records_4k | 36.9 | 9.80 | 9.73 | 11.22 | +0.07 |
| records_64k | 32.5 | 9.60 | 9.47 | 9.03 | +0.56 |
| varied_4k | 17.5 | 8.78 | 8.40 | 8.46 | +0.37 |
| varied_512 | 18.9 | 7.21 | 7.19 | 7.98 | +0.02 |

Two of them were below the noise floor, so level. What was being chased was
0.37 and 0.56 cycles.

### The change

The loop read the token with `ldrb` -- one byte. An `ldr w4` at the same
address costs the same and also brings, as long as the literal run is empty,
the offset (bits 8..23) and the match's length byte (bits 24..31). What that
replaces is two further dependent L1 accesses: the offset address is `token ->
shift -> add`, so three cycles behind the token, and the length byte three
behind that again -- on the chain that feeds the output cursor the next block
waits for. On top of that, these blocks no longer do the unconditional 16-byte
literal copy, which was overwritten anyway.

The census says how often: the literal run is empty in 88% of records_64k, 56%
of varied_4k, a third of the rest. In records_64k, 75% of those also saturate
the match nibble, so two thirds of its blocks take both out of this one load.

The catch that was feared -- a coin-flip branch on "literal run empty" for
varied -- did not materialise. The common case falls through rather than
branching, and 56/44 is apparently enough for the predictor.

### Measured, medians of 3 paired rounds, net of the controls

| foreign LZ4 | old | new | net | now vs liblz4 |
|---|---|---|---|---|
| varied_64k | 5.54 | 7.59 | **+36.7%** | +52.7% |
| varied_4k | 6.54 | 8.63 | **+31.5%** | +26.3% |
| records_64k | 11.08 | 13.56 | **+22.2%** | +20.9% |
| varied_512 | 8.62 | 9.16 | +5.7% | +6.0% |
| records_4k | 12.32 | 12.83 | +3.8% | +2.9% |
| noise_64k | 56.63 | 57.74 | +3.7% | +3.8% |
| noise_4k | 64.35 | 64.35 | +0.3% | -0.3% |
| noise_512 | 49.29 | 48.19 | -1.3% | +3.5% |
| records_512 | 11.48 | 11.11 | -3.2% | +1.0% |

**17 of 18 cells** in the foreign format, against 10 before. The one exception
is noise_4k at -0.3%, which is parity. Our own format is unchanged at 21 of 22
-- the change sits behind `#ifdef KEVA_LZ4` and all eleven rows move between
-2.3% and +1.2% net.

Correctness: 105 tests with `liblz4`, the soak's 5760 cases, and all nine
benchmark shapes byte for byte against what liblz4 packed.

### What that says about the rest of the day

Eleven ideas, nine of them zero or worse. The three that carried all did the
same thing: **removed work, not instructions.** memcpy took a loop away (+34%),
the padding took the checked path away (+8%), and this one takes two dependent
loads and a vector store away (+37%). Everything that instead tried to rebuild
the chain or save instructions cost between 0% and -31%.

## 2026-09-20 — The same trick in the even split

After the LZ4 body, our own format read `varied_4k` at 8.28 GiB/s while the
same kernel read a *foreign* LZ4 block of the same data at 8.63. The reason was
nothing but the guard: `varied` lands in our even split, which has the same
token layout as LZ4 and had not been given the trick. `#ifdef KEVA_LZ4` became
`#ifndef KEVA_WIDE`.

Medians of three paired rounds, net of the controls:

| own format | old | new | net | now vs liblz4 |
|---|---|---|---|---|
| varied_64k | 8.10 | 9.56 | **+18.3%** | +92.8% |
| varied_4k | 8.28 | 9.64 | **+16.4%** | +41.6% |
| noise_4k | 71.53 | 72.22 | +1.1% | +10.9% |
| records_32 | 6.12 | 6.15 | +0.6% | +87.0% |
| records_64k | 15.64 | 15.61 | -0.1% | +39.1% |
| varied_512 | 11.17 | 11.21 | -0.2% | +29.8% |
| records_4k | 15.96 | 15.89 | -0.3% | +28.0% |
| records_512 | 13.30 | 13.09 | **-1.5%** | +18.6% |
| records_192 | 9.62 | 9.32 | **-3.0%** | +43.7% |

All 22 of 22 cells stay won.

### The two negatives are real, and they have a reason

Confirmed over five paired runs each, control flat at +0.2%: records_192
-3.0%, records_512 -1.5%.

The census says why. The fast path falls *into* the special case rather than
branching to it, because an empty literal run is 88% of records_64k in LZ4's
format and 56% of varied. In *our* format, though, records carries one to three
literals in 56% of its blocks and none in only 32% -- there the ordering is the
wrong way round.

### Built the other way round, and it is worse

Special case taken out of the fallthrough and reached by a branch instead:

| | net |
|---|---|
| own records_192 | -3.0% -> -2.1% |
| foreign records_64k | **-7.9%** |
| foreign varied_512 | **-11.8%** |
| foreign noise_512 | **-11.6%** |

One arrangement cannot serve both distributions. The numbers pick the one that
stays.

### And why it cannot be made finer

The obvious move is to widen the special case to literal runs up to three --
that covers 88% of records and 76% of varied, both as fallthrough. It was
worked out and not built: the offset would then sit at a variable position, so
`ubfx` on the length, a shift, a mask, and the offset lands eight cycles behind
the token instead of four. Those four cycles are the entire gain. The
generalisation would throw away the thing it generalises.

## 2026-09-20 — The repeat bit back in the even split? Counted, not built

`records_512` was once 180 B and is 195 today. The fifteen bytes went when the
repeat bit moved from the even split into the wide split, because it cost 22.9%
of decode time there. The question was whether today's zero-literal path pays
that back.

It cannot, and the argument is about which blocks: the new path speeds up
blocks with a literal run of **zero**, while the price of 3/4/1 falls on blocks
with a literal run of **seven or more**. Disjoint.

The census over our own packed values (`examples/even_census.rs`):

| | blocks | chains today | chains then | new | share | repeats | size |
|---|---|---|---|---|---|---|---|
| records_192 | 4 | 2 | 2 | 0 | 0% | 1 | -2 B (-1.5%) |
| records_512 | 12 | 2 | 5 | 3 | 25% | 9 | **-15 B (-7.7%)** |
| varied_512 | 19 | 6 | 12 | 6 | 32% | 11 | -16 B (-4.5%) |
| varied_4k | 186 | 50 | 113 | 63 | 34% | 35 | -7 B (-0.3%) |
| varied_64k | 3054 | 738 | 1701 | 963 | 32% | 171 | **+621 B (+1.9%)** |

The 195 -> 180 agrees to the byte with the census of 09-18, which validates the
count.

**Two reasons not to build it.** A third of all blocks would newly jump out of
the loop into the literal chain -- exactly the mechanism that measured -22.9%
and -21.5%. And the size does not carry it: the gain exists only at 512 bytes,
falls to 0.3% at 4 KiB and reverses at 64 KiB, because the repeat rate falls
with size (5.6% at varied_64k) while the literal chain rises with it.

The same trade as on 09-18, this time worked out beforehand instead of measured
afterwards. That is the point at which a census replaces a measurement.

## 2026-09-20 — x86 gets the wide token load, and a test that checked nothing

The packer runs on x86; the decoders have none of the day's four gains. Only
one is ported for now -- the wide token load, worth between +22% and +52% on
aarch64 -- and on its own, because every answer on x86 costs a rented machine:
two changes in one round could not be told apart if the result came back bad.

The x86 site is the same one: `movzwl (%rsi,%r10,1), %ecx` loads the offset
from an address derived from the token, so load the token, shift, load the
offset. `mov (%r14), %eax` instead of `movzbl` brings token, offset and length
byte in one.

### The test that was green and checked nothing

Speed needs the cloud, correctness does not -- both targets build and test
here. Except: the existing overlap test derives its literal count from the
offset and therefore never reaches zero, so the new path was uncovered. The
newly written test was uncovered too, on the first attempt:

    DIAG decided=0 refused=208

None of 208 hand-built blocks were decoded at all -- the first token carried a
match nibble, but I had not written an offset behind it, and `continue` on
refusal swallowed that. A green test that executes nothing.

Two things fixed it. The block is now built validly (a seed run with a match,
then three blocks with no literals, then a literal tail), and the assertion is
`refused == 0` instead of `decided > refused`: a refusal is the kernel's
legitimate answer, but it is also how a broken fast path hides -- it shifts the
frame, the next block falls through a guard, and a test that only compares
bytes stays green.

### Cross-checked by mutation

| mutation | aarch64 | x86 |
|---|---|---|
| cursor step 3 -> 2 | FAILED | FAILED |
| offset from bit 8 -> 9 | FAILED | -- |
| extension byte bit 24 -> 25 | -- | FAILED |

Only that establishes that the test executes the path. Without the probe I
would have pushed an untested assembly path onto an architecture I cannot
measure.

Status: 106 tests on aarch64, 102 on x86, 5760 soak cases, clippy quiet. Speed
on x86 remains unmeasured -- that needs a round on Xeon and EPYC.

## 2026-09-20 — liblz4 built for x86 by hand, and what it found immediately

The soak with 5760 liblz4-packed values ran only on aarch64, because Homebrew
has no x86 liblz4 here. That left the x86 kernel covered by about two hundred
hand-built blocks and by nothing else. lz4 has no dependencies and builds in
four seconds:

    clang -arch x86_64 -O2 -c lz4.c lz4hc.c lz4frame.c xxhash.c
    ar rcs liblz4.a *.o

Measuring under Rosetta would be pointless; what executes is exactly the same
assembly, and correctness is precisely what Rosetta does not change.

First run against the freshly ported x86 kernel:

    5760 cases, 9 wrong

All nine **REFUSED**, none WRONG -- the kernel declines, the portable decoder
takes over, the output is right. Which is why every test stayed green.
Measured against `HEAD~1`: 0 wrong. So I put them there, within the hour.

### Bisected rather than guessed

Disable the shortcut for literal-less blocks, keep the wide load: **still
nine**. So it was not the shortcut but the load itself.

`L(dst_edge)` is the one exit from the loop header that *passes the token on*
in `%eax` instead of reloading it -- `L(src_edge)` and `L(fast_litend)` both do
their own `movzbl`, and the source even says why. With a dword load this path
carried three bytes of the next block into `L(slow)`, which shifted them as a
literal count.

One `movzbl %al, %eax` on a path that runs once per value: 0 of 5760.

### What that says about the day

Two bugs today, both of the same kind: an `#ifdef` branch is a second program,
and both times the second program broke an assumption that held tacitly in the
first. This morning's crash (tables in front of the `ret`) and this one. Both
found not because a test anticipated them, but because an *independent*
reference compared byte for byte.

## 2026-09-21 — The AVX threshold: built, measured, refuted

`noise_512` in the foreign format sits 30% behind liblz4 on Xeon. The profile
seemed to name the reason -- `perf annotate` put 78.9% of the kernel on two
instructions of the 64-byte literal loop, the *second* `vmovdqu` load and the
*second* store, while the first pair of the same iteration drew 0.00%. The size
series fitted: noise_64k +4.4% (1024 iterations), noise_4k +4.1% (64),
noise_512 -31% (8). A cost that dominates at eight iterations and vanishes at
sixty-four reads like a start-up cost.

So a threshold: runs under a kilobyte through the 16-byte loop, longer ones
through the 256-bit loop. Measured in both orders, because the 512-byte cells
had swung by thirty points over the evening:

| noise_512, GiB/s | position 1 | position 2 |
|---|---|---|
| without threshold | 35.23 | 35.20 |
| with threshold | 24.31 | **23.75** |

Position-independent and unambiguous: the wide loop is 32% better even at eight
iterations. Reverted.

`records_512` showed -10.2% in one position and +16.4% in the other -- the same
revision. That cell is unreadable without both orders, which the script header
has said for days and which here came close to buying a wrong verdict for the
second time.

### What remains

At 512 bytes it is about 36 cycles against liblz4's 25, and the copy itself is
only about ten of them. The rest is cost incurred once per value: twelve
push/pop, the zone setup, and above all the literal-length chain -- 512 bytes
need two extension bytes, two dependent L1 accesses back to back, on a value
that consists of exactly one block and therefore has nothing to overlap that
latency with.

### And a test gap that weighs more

The AVX store was deliberately corrupted -- the soak stayed green.
`Lz4Body::all()` returns only `[Baseline, Ssse3]` on a CPU without AVX2, and
under Rosetta there is no AVX2. So the Xeon and EPYC bodies **never** run on
this machine, neither in the soak nor in the tests. Repeated on the rented
machine: the same mutation drops the soak and two tests there. Those two bodies
can only be checked in the cloud.

## 2026-09-21 — The last literal run, without asking twice

Found by reading, not by measuring. `L(fast_litext)` reads the literal-length
chain, then discovers that the run reaches the end of the input -- `lea
2(%rsi,%r10,1) / cmp %r13 / ja L(fast_litend)` -- and jumps to `L(slow)`, where
the token is **loaded again**, the nibbles are **extracted again** and **the
same chain is walked a second time**.

For a value that consists of exactly one block, all of that is on the critical
path. 512 bytes of incompressible data is exactly that: a token, two extension
bytes, 512 literals. Four dependent L1 accesses where two would do.

At the bail-out point everything is already there: `rsi` past the length bytes,
`rdi` at the output, `r10` the length. What was missing were the two bounds the
fast path had not yet checked, and `r14` has to point at the run rather than at
the token, because `L(lit_done)` advances it by the run length. Eight
instructions instead of about thirty-five and three dependent loads.

### Measured, Xeon 8481C, four positions in both orders

Position 1 is discarded: the same revision read `noise_512` at 15.29 there and
at 35.14 GiB/s in position 3. The cold start hits whatever is measured first,
and the script header has said so for days.

| | old | new | delta | vs liblz4 before -> now |
|---|---|---|---|---|
| records_512 | 6.60 | 8.22 | **+24.7%** | -9.9% -> **+17.8%** |
| noise_512 | 35.14 | 40.68 | **+15.8%** | -31.0% -> **-20.1%** |
| noise_64k | 34.16 | 35.38 | +3.6% | -0.4% -> +3.0% |
| noise_4k | 67.77 | 69.19 | +2.1% | -0.0% -> +2.0% |
| records_64k | 11.50 | 11.50 | -0.0% | +31.5% |
| varied_64k | 4.65 | 4.66 | +0.2% | +14.5% |
| varied_512 | 6.06 | 6.11 | +0.8% | -0.6% |
| records_4k | 11.23 | 11.20 | -0.3% | +21.0% |
| varied_4k | 5.57 | 5.53 | -0.8% | +10.2% |

Seven of nine ahead of liblz4. The change costs nowhere more than one percent,
and is large on exactly the shapes made of a few long literal runs.

### What remains on noise_512

40.68 GiB/s is 31 cycles for 512 bytes; liblz4 needs 25. Six cycles, and the
copy itself is none of them -- eight iterations of 64 bytes each is about ten.
What remains is the frame and the zone setup, costs that a value made of a
single block cannot spread across several blocks.

aarch64 has the same duplication and is unmeasured.

### Correction to the entry above: records_512 shows nothing

The +24.7% on records_512 is not a result. Across every run of that day:

    8746808 (old)  8.06  8.50  6.78  6.60
    023e1c5 (new)  8.06  8.39

Both revisions lie in the same band of 6.6 to 8.5 GiB/s, a spread of 29%. That
the old revision happened to land at the bottom and the new one at the top in
that one run is the cell, not the change. The entry above reads +24.7%, and
that is wrong.

The only trustworthy part of that run is what repeats across several
measurements:

    noise_512   old  35.23  35.20  35.14     new  40.47  40.90   +15.6%
    noise_64k   old  34.16                   new  35.17  35.59    +3.6%
    noise_4k    old  67.77  67.77            new  70.61  67.77    +2.1%

So noise_512 moves from -31% to -20% against liblz4, and that holds. The other
six cells do not move.

This is the third time today that a 512-byte cell has nearly bought a wrong
verdict. For that size the rule is: at least two measurements per revision, and
where the bands overlap the result is "nothing" rather than the average.

## 2026-09-22 — The padding on x86: refuted, and a false premise with it

The question was why the Xeon kernel needs so many cycles. It needs none.
Cycles per block, computed from throughput and block count:

| | blocks | Xeon | M2 |
|---|---|---|---|
| records_64k | 1567 | **8.9** | 8.7 |
| records_4k | 96 | **9.2** | 8.7 |
| records_512 | 12 | 11.8 | 10.4 |
| records_192 | 4 | 36.9 | 16.2 |

At 1567 blocks, x86 is two percent off Apple silicon on identical source. The
loop is not the problem.

The outlier at records_192 was a single measurement, and the model did not
close: 148 cycles for four blocks against 141 for twelve is less time for three
times the work. A fixed base cost plus per-block work cannot produce that. Four
positions later records_192 reads 3.26 and 3.22 GiB/s -- stable, and the
outlier was the measurement.

### Built and reverted

The padding of the input remainder, worth +8.1% on exactly this shape on
aarch64, ported to x86. Four positions, warm against warm:

| | old | new | delta |
|---|---|---|---|
| same_bytes records_512 | 8.00 | 7.35 | **-8.1%** |
| own_format records_512 | 9.32 | 8.65 | **-7.2%** |
| same_bytes noise_512 | 45.99 | 43.18 | -6.1% |
| own_format varied_4k | 7.00 | 6.75 | -3.6% |
| same_bytes noise_64k | 35.39 | 34.25 | -3.2% |
| own_format records_192 | 3.26 | 3.22 | -1.2% |

The cell it was built for does not move, and records_512 loses eight percent.
Reverted.

**A suspicion that is not resolved.** own_format records_32 read 1.40 and 1.18
GiB/s -- cleanly separated by revision -- even though that cell does not touch
the kernel at all: at 32 bytes the packer refuses, the value is stored raw, and
the benchmark does a `copy_from_slice` there. A change to the assembly cannot
move that number. It moves anyway, so the changed object size shifts something
in the binary. That means part of the -7% and -8% above may be placement rather
than mechanics -- irrelevant to the verdict, since the change earns nothing
either way, but it does mean deltas of that size here cannot be attributed to
the logic alone.

### Where the two kernels have diverged

The same: two token splits as separate bodies, a dedicated LZ4 body, a zone
loop plus a checked one, shuffle tables for short overlaps, the COPY_MAX
ladder, memcpy for the last literal run.

Different, measured every time:

| | aarch64 | x86_64 |
|---|---|---|
| test for literal-less blocks | `cbnz w5`, the literal nibble only | `cmp $0x0F`, the whole token |
| memcpy threshold | from 8192 B | from 128 B |
| match loop | two forms | one |
| part lines | none | Xeon, EPYC, SSSE3 |
| input remainder padded | yes (+8.1%) | no (-8.1%) |

The first two contradict each other outright. The test on the literal nibble
was a gain on ARM and cost records_512 nine percent on Xeon, because there it
is a coin flip. And memcpy was 10.4% worse than the hand loop at 512 bytes on
ARM and 14.2% better on Xeon -- Apple's memcpy loses there, glibc's wins. Four
ideas were ported from one platform to the other today, and two of them changed
sign.

## 2026-09-22 — Four cells that never measured a decoder

The question was why records_32 in our own format sits 50% behind on Xeon,
when our decoder cannot run there at all. It did not run.

At 32 bytes the packer refuses -- `worth_storing` wants eight bytes saved and
12.5%, and LZ4 gets two out of 32 bytes of JSON -- and the value is stored raw.
For that case the benchmark measured

    mine[..data.len()].copy_from_slice(&data);

and called it "the read path". The store does not do that:

    if flags.compressed { pack::unpack(...); Cow::Owned(out) }
    else                { Cow::Borrowed(bytes) }

A raw-stored value is **borrowed, not copied** -- an arena slice, zero bytes.
So the cell put a copy that does not actually happen up against a real decode.

That the number was absurd was visible in the number itself: 1.40 GiB/s for 32
bytes is 57 cycles, and the same line measures 17.5 on M2. Identical Rust, sign
reversed -- +87% there, -50% here.

**And it does not affect only the bad cell.** The packer refuses four shapes:
records_32, noise_512, noise_4k, noise_64k. All four measured a memcpy in
`own_format`, and **three of them we "won" by a lot**: +76.6%, +34.2%, +3.5%
on Xeon. Of the 22 cells reported so far, eight were not decoder comparisons.

The benchmark now skips those shapes and says so:

    own_format records_32: packer refused, stored raw -- no decode to time

They stay in `pack/sizes` and `compress3`, where "stored raw" is the result
rather than an artefact.

### Our own format afterwards, M2 Max, medians of three

| | lzvolt | liblz4 | flex | vs liblz4 | vs flex |
|---|---|---|---|---|---|
| varied_64k | 9.72 | 5.08 | 7.83 | **+91.3%** | +24.1% |
| records_192 | 9.78 | 6.34 | 7.70 | +54.3% | +27.0% |
| records_64k | 15.87 | 11.23 | 11.60 | +41.4% | +36.8% |
| varied_4k | 9.71 | 6.87 | 7.08 | +41.3% | +37.1% |
| records_4k | 16.02 | 12.52 | 11.11 | +28.0% | +44.3% |
| varied_512 | 10.31 | 8.75 | 9.11 | +17.8% | +13.1% |
| records_512 | 13.01 | 11.12 | 10.76 | +17.0% | +21.0% |

14 of 14, and every one of them is a decode against a decode.

## 2026-09-22 — The last literal run, byte by byte: +119.5% on records_192

I had dismissed this cell twice as an unreliable measurement. It was stable and
it was right.

A cost model fitted per platform from records_512 and records_4k:

| | cycles per block | fixed per call | records_192 predicted | measured |
|---|---|---|---|---|
| Xeon | 8.9 | 31 | 67 | **148** (2.22x) |
| M2 | 8.4 | 28 | 61 | 64 (1.05x) |

The same shape fits the model on ARM and costs twice as much on x86. The
profile shows why in one line:

    26.39%  movzbl (%r9),%r14d
    10.87%  inc    %r9
     6.34%  inc    %rdx
     6.08%  dec    %rax
     5.95%  mov    %r14b,(%rdx)

55% of the kernel inside a byte-by-byte copy. It is reached as soon as a
literal run has fewer than thirty-two bytes of room left before the end of the
input -- because a block of that width would read past it. A ladder does not,
and one has stood a few hundred lines further down in the same file all along.

records_192 packs to 134 bytes in four blocks, so every run ending past byte
102 arrives there: half the file. records_512 packs to 195 in twelve, and only
the last one does. Hence 32 against 10 cycles per block.

### Measured, Xeon, four positions

| our format | old | new | delta |
|---|---|---|---|
| records_192 | 3.27 | 7.17 | **+119.5%** |
| varied_512 | 7.48 | 8.16 | +9.1% |
| records_512 | 9.22 | 9.45 | +2.5% |
| varied_4k | 7.01 | 7.17 | +2.3% |
| records_4k | 11.64 | 11.79 | +1.3% |
| records_64k | 11.93 | 11.83 | -0.8% |

| standard LZ4 | old | new | delta |
|---|---|---|---|
| varied_512 | 6.03 | 6.31 | +4.6% |
| records_4k | 11.03 | 11.12 | +0.8% |
| noise_512 | 45.93 | 44.94 | **-2.2%** |

7.17 against 7.17 in both new positions. records_192 therefore moves from
-23.0% to **+69.1%** against liblz4.

Standing on Xeon afterwards: our format **14 of 14**, standard LZ4 **16 of 18**.

### What the find says about the day

The cell was misfiled twice, both times by me, both times with an argument
instead of a measurement: first "512-byte cells fluctuate", then "the benchmark
is measuring its own harness". What settled it was a cost model built from two
other cells -- 148 cycles for four blocks against 138 for twelve is
arithmetically impossible, and that was visible before any machine ran.

## 2026-09-23 — The fast exit in the memcpy branch: refuted

Where the literal run finishes the value, the return needs no saved registers:
eight bytes of stack instead of twenty-four, four memory accesses fewer.
Measured on Xeon, four positions, `noise`:

| | P1 old | P2 new | P3 old | P4 new | old -> new |
|---|---|---|---|---|---|
| noise_512 | 45.28 | 43.42 | 45.17 | 43.27 | **-4.2%** |
| noise_4k | 66.88 | 66.38 | 66.81 | 66.41 | -0.7% |
| noise_64k | 36.59 | 34.04 | 34.87 | 34.12 | -3.6% |

Cleanly separated by revision. The `lea`/`cmp`/`jne` before every call costs
more than the two stores and two loads it saves, and the extra branch point
pulls the code apart. Reverted.

So noise_512 stays at -11.9% against liblz4 on Xeon, and the deficit is still
put at 3.4 cycles out of 28.7. On M2 we win the same cell at 34.3 against 34.7
cycles -- the difference being that glibc's memcpy gets 27% faster on x86 than
on ARM while our own version gets only 16% faster.

### On the procedure, because it cost money

The previous attempt at this measurement hung at 00:25 and was found at 09:49:
nine hours of instance time. Every status check in between asked whether the
process was alive instead of when the log file was last written. The re-run
therefore has three brakes -- a kill switch after forty minutes in the script
itself, `timeout 300` per measured cell, and a watchdog on the log file's
timestamp.

## All sixteen unpack shapes profiled, on EPYC and on Axion

`perf record` per shape, the symbol read out of `perf report` rather than
guessed, plus the ten hottest instructions from `perf annotate`. EPYC 9B14
(c3d-standard-4, europe-west4-a), Axion / Neoverse V2 (c4a-standard-4,
europe-west4-c). Both machines created for this one run and deleted afterwards.

The first attempt on Axion produced nothing at all: c4a does not accept
`pd-balanced` and needs `hyperdisk-balanced`, and the image has to be
`debian-12-arm64`. The bench script knows both; the profile script did not.

### Axion, our own format

| shape | kernel | hottest instruction | share | what it is |
|---|---|---|---|---|
| records_192 | 89.0% | `str q0, [x21, x24]` | 20.6% | the unconditional 16 B literal copy |
| records_512 | 96.5% | `stp q0, q1, [x13], #32` | 14.0% | the match copy |
| varied_512 | 94.6% | `stp q0, q1, [x13]` | **39.0%** | the match copy |
| records_4k | 77.2% wide | `stp q0, q1, [x13]` | 20.2% | the match copy |
| varied_4k | 99.4% | `stp q0, q1, [x13]` | **36.5%** | the match copy |
| records_64k | 97.9% wide | `stp q0, q1, [x13]` | 27.4% | the match copy |
| varied_64k | 99.0% | `stp q0, q1, [x13]` | **38.7%** | the match copy |

### Axion, foreign format

| shape | kernel | hottest instruction | share | what it is |
|---|---|---|---|---|
| lz4_records_512 | 98.3% | `stp q0, q1, [x13]` | 20.4% | match copy |
| lz4_varied_512 | 98.5% | `stp q0, q1, [x13]` | **43.1%** | match copy |
| lz4_records_4k | 98.3% | `stp q0, q1, [x13]` | 26.2% | match copy |
| lz4_varied_4k | 99.8% | `stp q0, q1, [x13]` | **43.8%** | match copy |
| lz4_records_64k | 98.6% | `stp q0, q1, [x13]` | 17.5% | match copy |
| lz4_varied_64k | 99.6% | `stp q0, q1, [x13]` | **45.7%** | match copy |
| lz4_noise_512 | 91.8% | `stp q0, q1, [x0], #32` | 45.6% | **our own 32 B literal loop** |
| lz4_noise_4k | 98.5% | `stp q0, q1, [x0], #32` | 57.7% | **our own 32 B literal loop** |
| lz4_noise_64k | 14.9% | `0x9d3f8` and others, 60% together | | **glibc memcpy** |

### EPYC, our own format

| shape | kernel | hottest instruction | share | what it is |
|---|---|---|---|---|
| records_192 | 77.9% | `shl $0x4,%ecx` | **23.4%** | index into the splat table |
| records_512 | 86.7% | `movdqu %xmm0,(%r8)` | 14.5% | literal copy; `shl $4` 11.8% |
| varied_512 | 89.5% | `movdqu 0x10(%rsi),%xmm1` | 15.0% | literal copy |
| records_4k | 82.3% wide | `add $0x20,%rsi` | 10.8% | match copy |
| varied_4k | 98.6% | `movdqu %xmm1,0x10(%r8)` | 18.2% | literal copy |
| records_64k | 97.8% wide | `add $0x20,%rsi` | 7.6% | match copy |
| varied_64k | 98.2% | `movdqu 0x10(%rsi),%xmm1` | 16.5% | literal copy |

### EPYC, foreign format

| shape | kernel | hottest instruction | share | what it is |
|---|---|---|---|---|
| lz4_records_512 | 96.2% | `shl $0x4,%ecx` | **25.4%** | index into the splat table |
| lz4_varied_512 | 98.2% | `movdqu 0x10(%rsi),%xmm1` | 9.7% | literal copy; `shl $4` 8.5% |
| lz4_records_4k | 98.0% | `movdqu 0x10(%rsi),%xmm1` | 9.9% | literal copy |
| lz4_varied_4k | 99.3% | `movdqu %xmm1,0x10(%r8)` | 19.5% | literal copy |
| lz4_records_64k | 98.4% | `movdqu 0x10(%rsi),%xmm1` | 29.2% | literal copy |
| lz4_varied_64k | 98.9% | `movdqu 0x10(%rsi),%xmm1` | 27.2% | literal copy |
| lz4_noise_512 | 33.3% | `pop %rbp` 18.5%, `push %rbp` 11.1% | | **prologue and epilogue** |
| lz4_noise_4k | 2.0% | `0x16db20` | **98.1%** | **glibc memcpy** |
| lz4_noise_64k | 8.1% | `0x16db75` | **89.4%** | **glibc memcpy** |

### What the profile proves

The unnamed entry appears on both machines exactly where that machine's memcpy
threshold falls -- on EPYC from 4k (`MEMCPY_MIN 128`), on Axion not until 64k
(`LIT_MEMCPY 8192`). That identifies it: it is glibc memcpy with the symbols
missing. The two machines confirm each other.

So every `noise` cell on both machines is the same decision, and it falls in
**opposite directions**:

- **Axion, noise_4k -33.1% and noise_512 -16.7%:** there we copy with our own
  32-byte loop, because `LIT_MEMCPY 8192` has not kicked in. The loop costs
  `ldp`, `stp`, `subs`, `cmp`, `cbz` per 32 bytes; glibc on Neoverse V2 runs 64
  bytes per pass with no comparison chain. At 64k, where memcpy does kick in,
  the deficit is gone. The 8192 threshold was measured on the M2 Max and is too
  high for Neoverse V2.
- **EPYC, noise_4k -7.9%:** glibc memcpy is already 98% of it there, and glibc
  loses to liblz4's wildcopy, which is allowed to overrun and handles no exact
  remainder. **noise_512 -22.4%** is not a copy at all: prologue and epilogue
  are 30% of kernel time, plus the memcpy call. At 512 bytes the fixed cost is
  the measurement. `MEMCPY_MIN 128` was measured on Xeon and is too low for
  Zen 4.

That falls out conveniently: x86 has its own part line per core, so
`MEMCPY_MIN` can be raised for EPYC without touching Xeon. **aarch64 has no
part lines** -- one body per split, shared between M2 Max and Neoverse V2. A
lower `LIT_MEMCPY` hits both. The first step is therefore a threshold series
locally on the M2 Max, which costs nothing: if the M2 is indifferent between
512 and 8192, the threshold falls globally and Axion gets its 33% for free. If
the M2 needs the 8192, it costs a part line on aarch64.

The second entry, which only EPYC has: `shl $0x4,%ecx` at 23.4% on records_192
and 25.4% on lz4_records_512, plus the `lea` onto the table. That is the splat
for matches with an offset below 16 -- a load from a 512-byte table whose
address depends on the offset, feeding a `pshufb`. On Zen 4 that is a
dependency chain of about six cycles, in the place where records is nearly
every block. Axion has nothing comparable at the same spot.

## A1: the LZ4 line's literal loop at 64 bytes, with a biased counter

The profile on Axion found 77% of the time on an incompressible 4 KiB block
inside five instructions, two of which asked the same question:

    L(lz4_lit_blk):
        ldp     q0, q1, [x1], #32       5.4%
        stp     q0, q1, [x0], #32      57.7%
        subs    x2, x2, #32            11.5%
        cmp     x2, #32                 8.5%
        b.hs    L(lz4_lit_blk)

`subs` subtracts and sets flags; `cmp` then asks the same question of the
result. Subtract the round width once on entry and the `subs` at the foot *is*
the test: the borrow means exactly "less than one round left". The `adds` on
the way out restores the true remainder and is at the same time the zero test a
`cbz` used to do. At 64 bytes per round instead of 32, five instructions per 32
bytes become three.

### M2 Max, four rounds, measured alternating

| cell | old | new | delta | all runs (GiB/s) |
|---|---|---|---|---|
| lzvolt/noise_512 | 46.54 | 49.78 | **+7.0%** | old 46.7 46.4 25.6 48.3 / new 49.7 49.9 50.4 49.7 |
| lzvolt/noise_4k | 64.15 | 69.20 | **+7.9%** | old 65.4 63.6 63.1 64.8 / new 69.1 68.5 69.3 69.5 |
| lzvolt/noise_64k | 55.54 | 55.69 | +0.3% | old 55.2 57.7 53.6 55.9 / new 57.6 56.6 54.3 54.8 |
| liblz4/noise_512 | 46.70 | 45.77 | -2.0% | old 47.2 47.5 26.4 46.2 / new 45.7 45.9 46.4 45.6 |
| liblz4/noise_4k | 64.10 | 63.79 | -0.5% | old 64.7 64.6 63.6 63.6 / new 62.0 64.7 64.1 63.5 |
| liblz4/noise_64k | 56.03 | 56.11 | +0.2% | old 55.1 56.5 55.5 58.7 / new 55.5 57.4 56.7 54.7 |

Round 3 hit both sides at once (25.6 and 26.4) -- a disturbance on the machine,
not an effect of the revision. Without it noise_512 stands at +6.4% against a
control drop of 2.0%, so real but weaker than the median says. noise_4k is
unambiguous: the distributions do not overlap.

**noise_64k at +0.3% is the control.** `LIT_MEMCPY 8192` applies there, the
loop is never entered, and the cell does not move. The gain sits exactly where
the theory puts it.

### That our own format cannot move

The loop sits under `#ifdef KEVA_LZ4`. Both revisions assembled with `clang -c`
and the objects compared:

| body | |
|---|---|
| `unpack.o` (even) | **identical**, 3320 B |
| `unpack_wide.o` | **identical**, 3400 B |
| `unpack_lz4.o` | changed, 4248 B |

### What the mutation test found

Four corruptions against the soak, each on its own:

| mutation | soak |
|---|---|
| second `ldp`/`stp` removed -- half the copy per round | **red** |
| `adds x2, x2, #48` instead of `#64` -- remainder 16 too small | **red** |
| entry bias `#32` instead of `#64` | green |
| `b.hs` -> `b.hi` | green |

`b.hi` is not a bug: at exactly 64 remaining the loop falls out and `L(exact)`
copies the 64 exactly. Correct output, only slower.

The entry bias is a bug. With `#32` the loop enters its body with forty bytes
remaining and then copies sixty-four -- **reading up to 63 bytes past the input
buffer**. The output stays correct anyway, because the extra bytes land in the
overrun slack the caller truncates regardless. The soak checks what comes out
and therefore cannot see it.

The version shipped is clean on that boundary -- a round runs only with
sixty-four bytes in hand and reads sixty-four. But the coverage for it is
missing, and it is missing for every path in this file, not just this one. A
test that places the packed block at the end of a page and leaves the next page
unmapped would report any overrun immediately with SIGSEGV. That is the next
gap to close.

## E1, E2a and A1 on rented hardware

Three machines at once, each created for this run and deleted afterwards: two
c3d-standard-4, because the script documents an 11% position penalty on the
512-byte cells for these parts and a pair therefore has to run in both orders,
and one c4a-standard-4 for A1. Three runs per revision, medians.

    A_amd_fwd   e227cfa  dann  91f5505
    A_amd_rev   91f5505  dann  e227cfa
    A_arm       280d5c7  dann  91f5505

### A1 on Axion: nothing

| cell | before A1 | after A1 | delta | liblz4 delta |
|---|---|---|---|---|
| noise_512 | 30.69 | 30.37 | -1.0% | -0.2% |
| noise_4k | 44.03 | 43.40 | **-1.4%** | -0.3% |
| noise_64k | 69.20 | 69.42 | +0.3% | +1.5% |
| records_512 | 10.23 | 10.16 | -0.7% | -0.1% |
| varied_512 | 7.17 | 7.16 | -0.1% | +0.6% |
| records_4k | 12.58 | 12.46 | -1.0% | -0.1% |
| varied_4k | 7.35 | 7.32 | -0.4% | +0.0% |
| records_64k | 14.22 | 14.28 | +0.4% | +0.2% |
| varied_64k | 6.50 | 6.46 | -0.6% | +0.5% |

Controls within 1.5%, most under 0.6%. **The M2 Max gains 7.9% on that same
loop, Neoverse V2 nothing.** The theory -- one redundant `cmp` per round -- was
right for the M2 and wrong for V2.

And it was wrong for the wrong reason. V2 writes 32 B/cycle; at 44 GiB/s on
4096 bytes we are at roughly 15.7 B/cycle, half of that, and liblz4 at 24.
Neither store-bound nor issue-bound -- striking two of five instructions moved
nothing. What remains is the **post-indexed addressing**: every `stp q0, q1,
[x0], #32` depends on the previous round's pointer writeback, so the rounds
cannot overlap. glibc addresses aarch64 with offsets from a base and one `add`
per round instead -- which is what the x86 body has done in `L(lit_l32)` for a
while, with a negative index counting towards zero. That is the next
hypothesis, and it has a mechanism rather than an instruction count.

A1 stays: +7.9% on one machine, -1.4% on the other against a -0.3% control.

### E1 on EPYC: noise_4k reaches parity

Read at equal positions and additionally normalised against liblz4, which
cancels the machine out:

| noise_4k | lzvolt | liblz4 | ratio |
|---|---|---|---|
| position 1, before E1 | 59.50 | 64.20 | 0.927 |
| position 1, after E1 | 64.01 | 65.48 | **0.978** |
| position 2, before E1 | 61.02 | 65.06 | 0.938 |
| position 2, after E1 | 65.63 | 64.20 | **1.022** |

Both positions agree in direction and magnitude: **from roughly 7% behind
liblz4 to parity.** That is the largest gap this run closes, and it came from
*using* a loop that had existed on this part line all along and was skipped by
a threshold measured on Xeon.

### E1 at noise_512: unreadable

| noise_512 | lzvolt | liblz4 | ratio |
|---|---|---|---|
| position 1, before E1 | 31.69 | 42.01 | 0.754 |
| position 1, after E1 | 34.10 | 44.79 | 0.761 |
| position 2, before E1 | 31.71 | 42.05 | 0.754 |
| position 2, after E1 | 28.15 | 39.87 | 0.706 |

Position 1 says nothing, position 2 says -4.8 percentage points. This is
exactly the cell the script documents the 11% position penalty for, and the
liblz4 control itself moves 12% between the two machines. The cell cannot be
answered from this run and needs one of its own, in both orders and over
several rounds.

noise_64k sits at 0.995 and 0.998 and does not budge -- correctly, since 65536
is still well above the new threshold and still goes to memcpy. Exactly 512 and
4096 are affected, and that is exactly where the movement is.

### E2a, the folded `lea`: nothing

| foreign format, position 1 | before | after | delta |
|---|---|---|---|
| records_512 | 9.27 | 9.28 | +0.1% |
| records_4k | 12.82 | 12.85 | +0.2% |
| records_64k | 13.15 | 13.05 | -0.8% |

The 4.5% from the profile did not turn into throughput. That fits the pattern:
shortening a dependency chain by one instruction buys nothing in this kernel as
long as the loop is not issue-bound -- the moved register add, the `tbnz` on
the token and the merged offset check all said the same. The change stays,
because it is strictly fewer instructions and costs nothing, but it is **not a
gain**, and the reasoning it was built on is refuted.

### And what it did not do: move our own format

E2a sits under `KEVA_SSSE3` and therefore also changes `unpack_epyc.o` and
`unpack_xeon.o`. Measured at equal positions:

| own format, position 1 | before | after | delta | liblz4 delta |
|---|---|---|---|---|
| records_512 | 10.68 | 10.68 | 0.0% | +0.9% |
| varied_512 | 9.06 | 9.12 | +0.7% | +0.7% |
| records_4k | 12.59 | 12.41 | -1.4% | +3.0% |
| varied_4k | 8.58 | 8.59 | +0.1% | +0.8% |
| records_64k | 12.06 | 12.17 | +0.9% | -0.2% |
| varied_64k | 8.40 | 8.34 | -0.7% | -0.4% |

Everything within 1.4% against controls that move by up to 3.0%. Our own format
holds.

## The alignment head, and where it belongs

Two c3d in reversed order, three runs per revision, plus one c4a.

### The decomposition that explains the rest of this section

Two sizes on the same machine separate fixed cost from throughput without
needing to know the clock: `t = F + n/B`, solved over 512 and 4096 bytes.

| | fixed cost | copy rate |
|---|---|---|
| EPYC lzvolt, without head | 8.84 ns | 86.4 GB/s |
| EPYC lzvolt, with head | 12.45 ns | **99.2 GB/s** |
| EPYC liblz4 | **4.51 ns** | 74.6 GB/s |
| Axion lzvolt | **5.36 ns** | 50.6 GB/s |
| Axion liblz4 | 6.75 ns | **81.4 GB/s** |

**The two parts have opposite problems.** On EPYC our stream is 16% better and
our entry twice as expensive; on Axion the entry is 20% cheaper and the stream
38% worse. That is why the same change has been measuring opposite signs on
these machines for weeks, and it is not noise but two different bets on the
size of the value.

From that follows the crossover against liblz4 on EPYC: **2365 bytes**. Below
it liblz4 wins by construction, because it leaves the bounds checks inside the
loop that we compute once on entry. The model hits both measured points to a
tenth of a percent -- -23.0% at 512 against a measured -23.4%, +5.6% at 4096
against a measured +5.7%.

### The head's threshold

The head costs 3.6 ns of fixed cost and raises the rate by 15%, so it only pays
for itself from **2417 bytes** upwards:

| run length | without head | with head | |
|---|---|---|---|
| 512 B | 14.77 ns | 17.61 ns | -16.2% |
| 1024 B | 20.69 ns | 22.77 ns | -9.1% |
| 2048 B | 32.54 ns | 33.10 ns | -1.7% |
| 2560 B | 38.47 ns | 38.26 ns | +0.6% |
| 4096 B | 56.25 ns | 53.74 ns | +4.7% |

128 was a guess and cost every 512-byte value 16%. 3072 rather than 2417,
because `noise_64k` moved 3% in both positions even though the head cannot run
there -- part of the 3.6 ns is code placement, so the crossover is a lower
bound.

### Measured, threshold 128 against 3072

| | old | new | raw | control | vs liblz4 |
|---|---|---|---|---|---|
| noise_512, pos 1 | 27.79 | 32.79 | **+18.0%** | +5.7% | 0.687 -> **0.767** |
| noise_512, pos 2 | 27.56 | 32.77 | **+18.9%** | +10.5% | 0.710 -> **0.764** |
| noise_4k, pos 1 | 72.86 | 72.06 | -1.1% | +0.0% | 1.118 -> 1.105 |
| noise_4k, pos 2 | 70.69 | 68.71 | -2.8% | +3.4% | 1.130 -> 1.062 |
| noise_64k | 42.07 | 41.83 | -0.6% | +0.2% | unchanged |

The liblz4 control itself wanders by 5-10% on noise_512, which is why the last
column counts. 0.69 to 0.766 in both positions, and that is where it stood
before the head (0.771 / 0.783): the threshold recovers what the head cost
there and still lets it run at 4 KiB, where it wins.

**EPYC therefore stands at -23.4% / +10.5% / +2.3% against liblz4 on the three
noise cells -- and the -23.4% is exactly the model's value. There is nothing
left to win on the copy there; the next inch comes out of the entry fee.**

### Refuted on Neoverse V2

The same head, on the part whose numbers the theory was derived from: noise_4k
**-3.3%**, noise_512 **-7.1%**, controls within 0.5%, everything else within
1.1%. Reverted. The two AArch64 bodies have disassembled to 556 identical
instructions line for line ever since.

The decomposition also says why the theory was wrong there: Axion streams at
50.6 GB/s against glibc's 81.4, while its fixed cost is below liblz4's. The
problem is the copy itself and not its alignment.

## A verdict in seven seconds, and what getting one used to cost

Up to this point, "did that help" was answered with the instrument built for
"what are the numbers": criterion, the full packer report, three runs, rented
hardware, forty minutes per revision. That is how an eighty-minute run came
about to find out whether one assembly loop had got faster.

`examples/quick.rs` answers only that one question. No framework between the
clock and the call: one loop, two time readings, nanoseconds per call -- the
unit the cost model is written in anyway.

### Four bugs, until it stopped lying

The first draft reported "BETTER" on five cells with unchanged code. Each of
the four findings below was a real measurement error; none of them was
statistics:

1. **Absolute nanoseconds compared across process boundaries.** Frequency and
   core assignment shift them by 5-8%. Now normalised against liblz4, measured
   in the same process.
2. **Measured one after the other instead of interleaved.** Three hundred
   milliseconds of our cell, then three hundred of liblz4 -- a frequency step
   fits in between, and the ratio then describes the step. Now alternating,
   median of five pairs.
3. **The round count was quantised to powers of four.** A cell near the
   boundary lands at 4^6 rounds in one run and 4^7 in the next: four times the
   loop length, four times the cache pressure. Constant within a run, different
   between runs -- exactly the shape of the phantom regressions. Now computed
   from a probe measurement.
4. **4K aliasing.** Aligning source and destination to 64 bytes was not enough;
   what also matters is their distance modulo 4096. Separate `Vec`s put that
   wherever the allocator happens to be -- stable within a process, different
   in the next. The same binary measured `lz4/varied_512` **32% apart**, five
   times in a row. Now one arena with fixed offsets and a 1088-byte stagger, so
   no two regions share a 4 KiB offset.

Point 4 is the same effect this tool exists to investigate -- the measuring
apparatus suffered from the problem it was built to measure.

### What it can do, cross-checked in both directions

| | |
|---|---|
| eight runs, code unchanged | **8x `= UNCHANGED`** |
| A1 surgically reverted | **3x `v WORSE`**: noise_512 -7.0 to -8.0%, noise_4k -6.7 to -7.2% |
| back to HEAD | `= UNCHANGED` again |
| runtime | **7 seconds** |

The forty-minute measurement had given A1 **+7.0%** and **+7.9%**. The tool
reads -7.0 to -8.0% and -6.7 to -7.2% for the reversal.

The resolution is measured per cell and never assumed better than 6%, because
that is what is left on a laptop. Nothing this project has actually won was
smaller than 8%.

## Aligning the source, on Neoverse V2: +49%

One c4a, seven minutes, and the only thing swapped was `unpack.S` between
`12741c5` and `9883466` -- everything else identical, both revisions on the
same machine in the same state, measured with `examples/quick`.

| cell | baseline | new | delta (three runs) | GB/s |
|---|---|---|---|---|
| lz4/noise_4k | 84.4 ns | **56.9 ns** | **+48.6 / +49.9 / +49.4%** | 48.5 -> **72.0** |
| lz4/noise_512 | 13.1 ns | **11.1 ns** | **+17.9 / +17.6 / +18.2%** | 39.0 -> **46.2** |
| lz4/noise_64k | 871 ns | 869 ns | +0.5% | unchanged |

Resolution 0.1 to 0.6% per cell. Against liblz4:

| | before | after |
|---|---|---|
| noise_4k | **-34.9%** | -3.2 / -2.3 / -2.7% |
| noise_512 | **-19.3%** | -4.8 / -5.1 / -4.6% |
| noise_64k | -3.3% | -2.8% |

From thirty-five percent behind to three. noise_64k does not move, which is the
control: `LIT_MEMCPY 8192` applies there, the head is never entered, and the
cell stays put. records and varied in the foreign format within 1.3%, our own
format within 1.8%.

**`own/varied_512` reads +5.0 to +7.0% and is not a gain.** The own-format body
is byte-identical between the two revisions; what moved is the layout of the
text section, because `unpack_lz4_neoverse.o` grew. Really measured, not
earned -- and an example of how much code placement is worth at this size.

### What that says about the method

The PMU counters predicted it: our loads 100% unaligned (44.7 billion against
1830 unaligned stores), glibc's exactly the other way round, and glibc reads
the same bytes 60% faster. Measured: 49%.

This is the day's first change where a theory said in advance what came out
afterwards. The ones before it -- the alignment head on the destination, the
folded `lea`, the 64-byte loop on V2 -- were *inferred* from the same numbers
and measured zero to negative. The difference is not more thinking; it is that
here a counter answered the question instead of my deriving it. The counter was
available the whole time and needed nothing but
`--performance-monitoring-unit=standard` when creating the instance.

### And the part line

It earns its keep now. Before this measurement `keva_unpack_lz4_neoverse` was
556 instructions byte-identical to the generic body and pure weight; now it is
the reason a Neoverse part aligns a different side than an Apple part does --
and the M2 body is untouched, which is checked against HEAD rather than
asserted.

## The light entry on EPYC: +61%, after a failure with a diagnosis

An incompressible value is a token with an empty match nibble, a length chain
and the bytes. The body has recognised that shape all along -- but only behind
six pushes and five `lea`s that compute zone boundaries and margins a
single-block value never reads, and it pays six pops on the way out.

### First attempt: -20.8%

| lz4/noise_512 | baseline | light entry |
|---|---|---|
| ns | 10.1 | 12.6 |
| three runs | | **-20.9 / -20.8 / -20.6%** |

The decomposition said two opposing items were inside that: the entry saved
about three nanoseconds, and the 32-byte loop with a comparison and a branch,
which I had written instead of `L(lit_last)`'s copy, gave back more than twice
as much.

### Second attempt, with that same copy: +61%

| lz4/noise_512 | baseline | light entry |
|---|---|---|
| ns | 10.0 | **6.4 - 6.7** |
| GB/s | 51.1 | **76.8 - 80.3** |
| vs liblz4 | **-8.0%** | **+39.2 / +43.1 / +48.1%** |
| delta | | **+61.0 / +55.5 / +51.3%** |

noise_4k moves 0.7%, noise_64k 0.5%, records_512 0.6% -- all three are above
the 3072 boundary and do not take the path. That is the control showing the
gain comes from the place that was touched.

Checked on the part itself, because it does not run here: Rosetta has no AVX2,
`Lz4Body::all()` holds the EPYC bodies back, and the local soak never reaches
this code. On the c3d: **5760 cases, 0 wrong**, 106 tests. And only
`unpack_lz4_epyc.o` moves -- `unpack_epyc`, `unpack_wide_epyc`, `unpack_xeon`,
`unpack_lz4_xeon` and `unpack_ssse3` are byte-identical.

### And a correction to an earlier number

The baseline here reads **-8.0%** against liblz4 where criterion had measured
**-23.4%**. The difference is buffer placement: `examples/quick` puts source
and destination in an arena at fixed offsets, criterion takes what the
allocator gives. A third of what counted all day as a structural deficit was
the measuring apparatus.

That also puts this morning's arithmetic in perspective: it said the cell sat
at -23.4% on the design's floor, because the two-parameter model predicts
-23.0%. The model was right for *that* measurement. The floor was somewhere
else.

## The light entry on all four parts

A value consisting of a single literal run is answered before the prologue.
Measured with `examples/quick`, three runs per part, only `unpack.S` swapped
between the revisions.

| part | before | after | delta | vs liblz4 |
|---|---|---|---|---|
| EPYC | 10.0 ns | 6.4-6.7 | **+61 / +56 / +51%** | -8.0% -> **+39 to +48%** |
| Xeon | 9.3 ns | 7.1-7.2 | **+29.2 / +30.1 / +29.9%** | -2.8% -> **+25.6 to +26.5%** |
| M2 Max | 9.1 ns | 7.3 | **+26.1 / +25.5 / +25.3%** | +4.2% -> **+30.5 to +31.4%** |
| Axion | 11.6 ns | 10.9-11.1 | +5.9 / +5.8 / +6.6% | -8.3% -> -2.2 to -2.9% |

noise_4k and noise_64k move by under 1.2% on all four: they are above the 3072
boundary and do not take the path. Correctness on Xeon and EPYC, 5760 soak
cases each, 0 wrong, 106 tests. All ten own-format bodies are byte-identical on
both architectures.

### Why Axion gets so much less

There the baseline already carries the source-alignment head, and that is
fixed cost in itself. The light entry saves the prologue but does not align --
so it trades one gain against the other, and six percent is left on a
resolution of six. Two of the three runs therefore say "unchanged". Pulling the
source head *into* the light entry instead of bypassing it would be the fix.

### What went wrong here methodologically

I had excluded M2 and Axion on the grounds that their fixed cost was already as
low as liblz4's. That was the wrong question. The right one is: how much of a
512-byte call is fixed cost that a shortcut can remove -- EPYC 48%, Axion 41%,
M2 30%. On EPYC we were actually *cheaper* than liblz4 on entry and still won
61%.

And the number the reasoning rested on (EPYC F = 8.84 ns) came from the
criterion data, whose buffer placement had been shown to distort on that same
day. With pinned buffers it is 4.76 ns, and after the light entry 0.71.

The objection came from him: the code is the same on every architecture, so the
problem must be the same everywhere. He was right.

### A mistake while building it, and how it surfaced

The first insertion on AArch64 silently missed -- a text replacement with no
match, because there are comment lines between the label and the prologue.
Tests and soak ran green, but over unchanged code. Four mutations survived,
which was already the hint; a `brk` at the spot never fired, which proved it.
Inserted afterwards with a tool that fails loudly on a miss.

## Axion, the last test: making the two gains add instead of cancel

The light entry jumped over the prologue and therefore also over the
source-alignment head, which sits in the full path's literal loop -- a value
short enough for the entry never reaches it. That is why it brought 6% on Axion
where Xeon and M2 got 26 to 30%. Now the entry does both.

| lz4/noise_512 | without light entry | with both |
|---|---|---|
| ns | 11.1 | **7.7 - 7.8** |
| GB/s | 46.2 | **65.5 - 66.1** |
| vs liblz4 | -4.7% | **+35.4 to +36.2%** |
| delta | | **+42.1 / +42.9 / +42.4%** |

noise_4k and noise_64k move by under 1.2%; both are above the boundary.

The generic AArch64 body is untouched by this: its instruction stream is
identical, and the object differs by one local label in the symbol table.

## Where the decoder stands after this day

`lz4/noise_512`, the cell that was behind on three of four parts in the
morning:

| part | morning | evening |
|---|---|---|
| EPYC | -8.0% | **+39 to +48%** |
| Xeon | -2.8% | **+25.6 to +26.5%** |
| M2 Max | +4.2% | **+30.5 to +31.4%** |
| Axion | -15.9% | **+35.4 to +36.2%** |

Axion's `noise_4k` went from **-34.9% to -2.3%** on the same day, EPYC's from
-7% to +23.8%.

### And the product

For a cache, capacity and throughput are independent axes: more values per GB
of RAM, and every value read faster. Our own format against liblz4 on its own:

| shape | capacity | M2 Max | Xeon | EPYC | Axion |
|---|---|---|---|---|---|
| records_4k | +35.1% | +67.4% | +58.1% | +48.6% | +75.4% |
| **records_64k** | **+44.7%** | **+101.5%** | **+96.3%** | +79.1% | **+106.6%** |
| varied_4k | +4.7% | +58.6% | +46.9% | +42.1% | +60.3% |
| varied_64k | -1.4% | +89.1% | +71.4% | +62.6% | +83.4% |

On records_64k, twice the logical bytes per second per GB of RAM.

## The standardised run on `ad7d338`

Three machines, three runs per machine, the full test matrix before measuring,
the full filter. 46 minutes. Medians of three.

### First: a test that had been red for a day

On ARM, `the_kernel_itself_reads_what_liblz4_wrote` failed -- not a kernel bug
but the guard check inside it:

    assertion `left == right` failed: a body was added or removed
    without this test noticing.   left: 2   right: 1

The test counts the LZ4 bodies and was pinned to one on AArch64. The Neoverse
part line made it two. **It had been red since `3bf7bb4`, so since that
morning, and went unnoticed for a full working day** -- because the check loop
after every change ran `cargo test` without `--features liblz4` and therefore
never executed the four interop tests.

That is exactly what the long run exists for: four configurations instead of
one.

### Packing, GiB/s

| shape | Xeon | | EPYC | | Neoverse V2 | |
|---|---|---|---|---|---|---|
| | lzvolt | vs | lzvolt | vs | lzvolt | vs |
| records_512 | 2.00 | **+55%** | 2.62 | **+122%** | 2.23 | **+54%** |
| varied_512 | 1.25 | **+58%** | 1.55 | **+89%** | 1.22 | +28% |
| noise_512 | 2.43 | **+81%** | 2.95 | **+134%** | 2.87 | **+77%** |
| records_4k | 2.78 | +19% | 3.60 | +29% | 3.33 | +22% |
| varied_4k | 1.15 | +20% | 1.47 | +27% | 1.16 | -2% |
| noise_4k | 7.16 | **+70%** | 7.74 | **+76%** | 10.34 | **+99%** |
| records_64k | 3.14 | +25% | 3.83 | +20% | 3.56 | +20% |
| varied_64k | 1.25 | **+92%** | 1.08 | +0% | 1.08 | +4% |
| noise_64k | 114.6 | **+859%** | 125.6 | **+820%** | 165.1 | **+837%** |

### Decoding, our own format, GiB/s

| shape | Xeon | | EPYC | | Neoverse V2 | |
|---|---|---|---|---|---|---|
| records_512 | 9.44 | **+29%** | 10.78 | **+33%** | 10.60 | **+22%** |
| varied_512 | 8.24 | **+29%** | 9.10 | **+28%** | 8.47 | **+27%** |
| records_4k | 11.71 | +18% | 12.63 | +14% | 13.14 | **+32%** |
| varied_4k | 7.08 | **+41%** | 8.40 | **+37%** | 8.32 | **+54%** |
| records_64k | 11.86 | **+36%** | 12.25 | **+23%** | 12.93 | **+45%** |
| varied_64k | 7.01 | **+72%** | 8.40 | **+64%** | 8.15 | **+83%** |

**Eighteen of eighteen cells ahead, +14% to +83%.**

### Decoding, identical LZ4 blocks, GiB/s

| shape | Xeon | | EPYC | | Neoverse V2 | |
|---|---|---|---|---|---|---|
| records_512 | 8.49 | **+25%** | 9.22 | **+28%** | 10.00 | +18% |
| varied_512 | 6.21 | -3% | 6.94 | -3% | 7.01 | +5% |
| noise_512 | 43.17 | **-15%** | 35.76 | **-12%** | 38.15 | +4% |
| records_4k | 11.23 | +14% | 12.70 | +14% | 12.74 | **+29%** |
| varied_4k | 5.58 | +11% | 6.87 | +10% | 7.35 | **+36%** |
| noise_4k | 66.38 | -2% | 70.93 | +9% | 64.84 | -3% |
| records_64k | 11.63 | **+33%** | 13.17 | **+31%** | 14.22 | **+60%** |
| varied_64k | 4.67 | +15% | 5.85 | +15% | 6.51 | **+46%** |
| noise_64k | 35.07 | +2% | 41.39 | +1% | 68.92 | -4% |

### And where this run contradicts the short tool

`noise_512` in the foreign format reads -15% / -12% / +4% here, where
`examples/quick` measured +26% / +39% / +36%. The difference is buffer
placement, and it is already documented: `quick` puts source and destination in
an arena at fixed offsets, criterion takes what the allocator gives. At nine
nanoseconds per call, a thirty percent spread has been measured from 4K
aliasing alone.

Both numbers are right for their regime, and the conclusion is uncomfortable:
**our kernel is far more placement-sensitive than liblz4 on this cell.** The
light entry does *not* align on x86 -- only the Neoverse body does, and that is
precisely the one body that is not negative on noise_512 in the foreign format.
That is a hint with a mechanism behind it, and the next step if work continues
here.
