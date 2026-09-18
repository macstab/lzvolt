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

Short form, M2 Max, baseline `vor` = `12760b2` at the same measurement time.
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
