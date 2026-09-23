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

## 2026-09-20 — memcpy im LZ4-Literalpfad: +34.4% auf noise_64k

Die Frage war "wieso machen wir im Falle von noise keinen memcpy?" — und die
Antwort auf den ersten Versuch von gestern: wir haben, aber an der falschen
Stelle. Der Aufruf stand in `L(slow_lit_wide)`, und ein 64-KiB-Literallauf wird
dort abgewiesen (`x16 + 32 > x20`, weil ein Lauf bis ans Eingabeende keine 32
Bytes mehr hinter sich hat). Er landet in `L(slow_lit_exact)` und damit in
`L(lz4_lit_blk)`. Der Aufruf ist nie gelaufen.

Das Profil nach dem Ketten-Fix zeigt genau dort die Zeit, 5076 Samples:

    .Ll_lz4_lit_blk    4377   86.2%
    .Ll_litlen_byte     637   12.5%

### Die Schwelle, gemessen statt geschätzt

`same_bytes/keva/noise_*`, M2 Max, Criterion 2 s, Mediane aus drei, Basis im
selben Lauf gemessen. Drei Schwellen nacheinander:

| LIT_MEMCPY |  512 B |  4 KiB | 64 KiB |
|------------|--------|--------|--------|
| 512        | −10.4% |  −0.2% | +26.7% |
| 4096       |  +0.3% |  −1.8% | +36.8% |
| **8192**   |  −0.1% |  +0.8% | **+34.4%** |

Ein Aufruf kostet etwa so viel, wie die Schleife auf 512 Bytes gewinnt, und bei
4096 stehen beide gleich — aber nur, wenn 4096 noch *in* die Schleife fällt.
Mit der Schwelle auf 4096 nimmt ein 4-KiB-Wert den Aufruf gerade eben und
verliert 1.8%; darüber ist die Zeile sauber. Deshalb 8192 und nicht 4096.

Absolut, gegen die Kontrollen im selben Lauf: noise_64k 42.5 -> 57.2 GiB/s,
liblz4 57.4, lz4_flex 40.1. Aus −26.5% wird Parität.

### Dass unser Format sich nicht bewegt, ist nicht gemessen, sondern gezeigt

`objdump -d` über `unpack.o`, Symbol für Symbol vor und nach der Änderung: alle
55 `.Le_*` (even) und alle 47 `.Lw_*` (wide) sind instruktionsgleich. Verändert
hat sich `.Ll_slow_lit_exact` (2 -> 8) und neu ist `.Ll_lz4_lit_loop`. Ein
Durchsatzvergleich hätte hier nur das Rauschen der Maschine gezeigt — in
denselben Läufen bewegte sich lz4_flex auf `varied_512` um 9.3%.

### Offen

x86_64 hat für diesen Pfad schon `rep movsb` hinter `REP_MIN 1024`, aber nur
auf der Xeon-Linie; EPYC hat ihn bewusst nicht, weil Zen ERMSB ohne FSRM hat.
Ob dort ein `memcpy`-Aufruf dasselbe tut wie hier, ist ungemessen.

## 2026-09-20 — Der lange Match: vier Instruktionen statt acht, aber nicht ueberall

Das Profil nach dem memcpy-Commit zeigt, dass alle drei verlierenden
Fremdformat-Zellen vollstaendig in der schnellen Schleife sitzen -- kein
langsamer Pfad mehr, und `rep_checked`, das bei `lz4_records_4k` einmal 47.1%
war, taucht gar nicht mehr auf:

    lz4_varied_4k     fast 33.4%  fast_offset 32.9%  fast_copy 10.4%
    lz4_records_64k   fast 24.8%  fast_lendone 23.2%  fast_offset 22.1%
                      fast_longout 8.9%  fast_longblk 8.2%
    lz4_records_4k    fast 25.8%  fast_lendone 21.1%  fast_offset 19.3%

`fast_longblk` und `fast_longout` zusammen sind 17.1% bzw. 14.2%, und die
Schleife trug dieselbe Buchfuehrung, die bei `litcp_block` schon einmal
herausflog: Zaehler, Vergleich, zwei Zeigeradditionen und ein unbedingter
Ruecksprung um zwei Moves herum. Post-Index und `subs` machen aus acht vier.

### Zwei Aenderungen, ein Messfehler

Zuerst beide Blockschleifen umgebaut, die Match- und die Literal-Variante.
Mediane aus drei, Basis im selben Session-Block:

| eigenes Format | beide | nur Match |
|---|---|---|
| records_4k  | +5.2% | +5.6% |
| records_512 | +4.5% | +5.0% |
| records_64k | +5.0% | +4.0% |
| records_192 | **-3.7%** | +1.1% |

Die Literal-Haelfte war der Schaden und ist zurueckgenommen. Warum sie schadet,
ist nicht gemessen; dass sie schadet, dreimal.

### Und eine Schleife in zwei Formen

Die Match-Haelfte allein gewann unser Format und verlor das fremde:

    own_format/keva/records_64k   14.82 -> 15.47   +4.4%   Kontrollen +1.0%
    same_bytes/keva/records_64k   11.11 -> 10.76   -3.2%   Kontrollen +1.2%

Also beide Formen behalten, `#ifdef KEVA_LZ4` dazwischen. Der Grund ist
Platzierung, nicht Arbeit: die Adressen, die Bytes und die Iterationszahl sind
identisch, aber der LZ4-Body schickt 99% seiner records-Bloecke hier durch und
unserer ein Fuenftel, und sechzehn Bytes Code weniger vor einer so heissen
Schleife sind nicht gratis. Zwei Formen fuer 7.7% gemessene Differenz.

### Stand danach, Mediane aus drei, netto gegen die Kontrollen

| eigenes Format | delta | netto |  | fremdes LZ4 | delta | netto |
|---|---|---|---|---|---|---|
| records_4k  | +6.8% | +6.5% | | records_4k  | +0.5% | +0.7% |
| records_512 | +5.3% | +6.6% | | records_512 | +0.1% | +0.6% |
| records_64k | +4.2% | +4.2% | | records_64k | +0.3% | +0.5% |
| noise_64k   | +0.9% | +3.2% | | noise_512   | -1.7% | -1.8% |

Eigenes Format 22 von 22 Zellen. Fremdes LZ4 unveraendert 10 von 18; der
groesste Rueckstand ist jetzt varied_4k mit -4.4% gegen liblz4.

x86_64 hat dieselbe Schleife und ist ungemessen.

## 2026-09-20 — varied im Fremdformat: der Zensus, und eine widerlegte Idee

Der Auftrag war der Flow von varied_4k und varied_512 im fremden LZ4-Format,
wo wir -4.4% und -2.6% gegen liblz4 liegen. Ein Zensus ueber den LZ4-Block
zaehlt, was der Decoder je Block gefragt wird -- und daneben, wie oft die
Antwort sich gegenueber dem Block davor aendert, denn das ist, was der
Praediktor sieht:

| | varied_512 | varied_4k | records_4k | records_64k |
|---|---|---|---|---|
| Bytes je Block | 18.9 | 17.5 | 36.9 | 32.5 |
| Literale / Match | 2.6 / 15.3 | 2.0 / 15.5 | 2.2 / 34.7 | 0.2 / 32.3 |
| Matchnibble gesaettigt | 11.1% | 24.8% | 76.6% | 75.0% |
| davon Wechsel | 22.2% | 34.2% | 38.7% | 49.2% |
| Matchlaengen 4-14 / 15-31 | 73% / 27% | 49% / 47% | 25% / 47% | 25% / 47% |
| Literallaengen = 0 | 33% | 56% | 32% | 88% |

Zwei Dinge, die records nicht hat. varied traegt **17.5 Bytes je Block**, halb
so viel wie records, und seine Matchlaengen liegen **auf der Schwelle**: die
Saettigung beginnt bei 19 Bytes, und 49% liegen darunter, 47% darueber. Der
Sprung `cmp w26,#15 / b.eq fast_long` wechselt bei jedem dritten Block die
Richtung und ist damit nicht vorhersagbar.

### Die Rechnung, die den Rest erklaert

Der Loop-Kopf sagt, die Schleife laeuft bei 8.7 Zyklen je Block. 17.5 Bytes
durch 8.7 Zyklen bei 3.5 GHz sind **6.5 GiB/s**, und varied_4k misst 6.48.
Die Form ist also vollstaendig durch die Token-Kette gebunden, nicht durch
Arbeit -- liblz4 liegt bei denselben Bloecken auf etwa 8.3 Zyklen.

### Und die Idee, die daraus folgte und nicht stimmt

Die Kette ist `ldrb` (4) -> `lsr` (1) -> `add x8` (1) -> `add x23, #2` (1).
AArch64 kann Schieben und Addieren in einem Befehl, also `add x8, x7, x4,
lsr #LIT_SHIFT` statt `lsr` + `add` -- auf dem Papier ein Zyklus von sieben,
gut 11%.

Gemessen, zwei Laeufe, gegen die Kontrollen:

| | fremdes LZ4 | eigenes Format |
|---|---|---|
| records_4k | -5.9% | -6.0% |
| records_64k | -5.6% | -6.5% |
| varied_4k | -4.1% | -5.2% |
| varied_64k | -9.7% | -6.1% |

Die geschobene Addition ist auf M2 nicht ein Zyklus, sondern zwei, und der
`lsr` musste ohnehin stehenbleiben -- der Vergleich und der Ausgabecursor
wollen die Zahl. Also eine Instruktion mehr und kein Zyklus weniger.
Zurueckgenommen.

Damit ist die achte Idee an derselben Stelle gescheitert, und alle acht haben
versucht, die Kette zu verkuerzen oder Instruktionen zu sparen. Was in diesem
Kernel je gewirkt hat, war etwas anderes: mehr Bytes je Block (wide split,
lazy matching, je ~19%) oder weniger Arbeit ueberhaupt (memcpy, +34%). Beim
fremden Format ist die Blockgroesse nicht unsere Entscheidung.

## 2026-09-20 — Der kurze gesaettigte Match: gebaut, gemessen, null

Vorgerechnet war es null und gemessen ist es null, aber gerechnet hatte ich mit
Instruktionen, und die sind in diesem Loop nicht die Waehrung -- also gebaut.

Das Nibble saettigt bei neunzehn Bytes, der feste Move schreibt zweiunddreissig.
Jeder Match dazwischen lief durch Laengenbyte, Waechterkette und Blockschleife,
um dieselben zweiunddreissig Bytes geschrieben zu bekommen, die `L(fast_copy)`
in einem Speicherpaar erledigt. Auf varied ist das ein Fuenftel aller Bloecke.
Zwei Instruktionen dazu (`cmp x14,#COPY_MAX` / `b.ls L(fast_copy)`), etwa fuenf
gespart, nur unter KEVA_LZ4 -- der wide-Split schreibt achtundvierzig, und die
Marge, die `L(fast_litlong)` prueft, sind zweiunddreissig.

Mediane aus drei gegen eine Basis aus sechs Laeufen mit unveraendertem Code:

| fremdes LZ4 | netto | | eigenes Format | netto |
|---|---|---|---|---|
| varied_4k | +0.6% | | records_4k | -0.4% |
| varied_512 | -0.8% | | records_64k | +0.4% |
| records_4k | -0.8% | | varied_512 | -3.2% |
| records_64k | -1.3% | | noise_512 | +3.1% |

Zurueckgenommen. Die neunte Idee an dieser Stelle, und die erste, deren
Ergebnis vorher ausgerechnet war -- fuenf Instruktionen auf einem Fuenftel der
Bloecke sind 0.5% der Instruktionen, und eine gesparte Instruktion kauft hier
keinen Zyklus. Das steht seit dem `ccmp`-Versuch im Kopf von unpack.S und gilt
weiter.

### Was der Lauf nebenbei geeicht hat

`own_format/varied_512` misst -3.2%, obwohl die Aenderung hinter `#ifdef
KEVA_LZ4` steht und unsere Bodies sie nicht sehen. Ein Median aus drei traegt
auf dieser Maschine also rund 3% Rauschen, und alles darunter ist keine
Messung. Die Kontrollen in denselben Laeufen lagen bei +-0.5%, weshalb die
Netto-Spalte und nicht die Delta-Spalte zaehlt.

## 2026-09-20 — Der Decoder fuehrte seine eigenen Tabellen aus

Kein Messergebnis, ein Fehler, und gefunden beim Absichern einer Messung.

Das Epilog der Wiederhol-Routine stand hinter dem `#endif`, das die
Shuffle-Masken beendet:

```asm
L(rep_ret):
#ifdef KEVA_LZ4
    ... 512 Bytes Masken ...
#endif
    ldp     x29, x30, [sp], #32
    ret
```

In jedem Body ohne die Tabellen erreicht `L(rep_ret)` das `ret` direkt. Im
LZ4-Body liegen fuenfhundertzwoelf Bytes Daten dazwischen, und jeder Ruecksprung
aus der Doppelungsleiter hat sie als Code ausgefuehrt. `EXC_BAD_INSTRUCTION` in
`.Ll_splat_first`, also mitten in der ersten Maske.

Ausgeloest von jedem fremden LZ4-Block mit einem Match, das weiter zurueckreicht
als es lang ist, bei Offset sechzehn oder mehr -- unter sechzehn faengt der
Shuffle-Pfad davor ab. Ein 63-Byte-Wert mit Periode 23 reicht:

    declared=63  block=38 bytes  ->  SIGILL

Warum niemand es gesehen hat: die Tabellen stehen hinter `#ifdef KEVA_LZ4`, und
alle Tests dekodieren unser eigenes Format. Die Benchmark-Shapes treffen die
Bedingung nicht -- `records`, `varied` und `noise` haben in liblz4s Ausgabe
keinen ueberlappenden Match bei Offset >= 16. Hundert gruene Tests und achtzehn
gruene Benchmarkzellen, und der Decoder stuerzte auf der ersten Datei ab, die
sich anders wiederholt.

Der x86-Body hatte es nie: dort steht `L(splat_ret): ret` vor den Tabellen. Der
aarch64-Port hat beim Uebernehmen das `ret` verloren.

Dazu zwei Tests. `the_kernel_reads_a_match_that_overlaps_itself` baut Bloecke
von Hand -- Offsets 1 bis 40, Laengen bis 300 -- und laeuft damit auf jedem Body
und ohne liblz4; ohne den Fix endet er mit SIGILL. Und `examples/lz4_soak.rs`
schickt 5760 von liblz4 gepackte Faelle durch den Kernel und vergleicht Byte
fuer Byte.

Die Lehre ist nicht "mehr Tests". Es ist, dass eine `#ifdef`-Variante ein
zweites Programm ist, und dass dieses zweite Programm hier nur an einem
Benchmark haengt, der Durchsatz misst und keine Korrektheit.

## 2026-09-20 — Das Laengenbyte gratis, und die zehnte Widerlegung

Das Profil mit feineren Labels, `varied_4k`, 6728 Samples:

    .Ll_fast        22.1%   die zwei Zonen-Vergleiche, also der Ruecksprung
    .Ll_px_guard    20.3%   die fuenf Befehle direkt hinter b.eq L(fast_long)
    .Ll_fast_offset 12.9%   ldrh Offset und Cursor
    .Ll_px_token    12.3%   Token laden, schieben, Nibbles
    .Ll_fast_copy   11.1%   der feste Move

`px_guard` sind fuenf billige ALU-Befehle, 12% der Instruktionen und 20% der
Zeit, und sie liegen im Landepunkt des Sprungs, den der Zensus mit 34%
Richtungswechsel als unvorhersagbar ausgewiesen hat.

`varied_512` zeigt daneben 20.5% auf dem geprueften Pfad (`slow_lit_ready` 8.0,
`slow_mat_ready` 4.5, `slow_lit_done` 3.7, `tail` 2.9, `slow` 1.4) -- das ist
der Eingabe-Rand von siebzehn Bytes auf einem Wert, der in rund 27 Bloecke
zerfaellt. Unangetastet.

### Die Idee

Das Byte, das einen gesaettigten Match verlaengert, liegt zwei hinter dem
Offset. Ein 32-Bit-Load an derselben Adresse traegt beide und kostet dasselbe
wie der 16-Bit-Load, den er ersetzt -- kein zweiter Load, keine zweite Adresse,
und genau darin unterscheidet er sich von dem Versuch, der im Quelltext mit
-18% dokumentiert ist. Die Laenge wird dann mit `csel` gebildet und der Sprung
auf "passt in den festen Move" ersetzt den auf "Nibble gesaettigt": 3% statt
25%, und vorhersagbar.

### Zweimal gemessen, zweimal dasselbe

Beim ersten Mal fiel `b.hi L(fast_lenbig)` in sein eigenes Ziel, also lief
jeder Block durch die Blockschleife -- varied -26%. Das Label aus dem Fallweg
genommen und noch einmal, Basis drei Laeufe, neu zwei:

| fremdes LZ4 | netto |
|---|---|
| varied_512 | **-30.8%** |
| varied_64k | -22.3% |
| varied_4k | -18.7% |
| records_64k | -9.8% |
| records_4k | -5.7% |
| noise_512 | +2.2% |

Der Grund steht im Diff: `cinc x23, x23, eq`. Vorher hing der Eingabecursor des
naechsten Blocks nur an der Literalzahl -- `ldrb` -> `lsr` -> `add x8` ->
`add x23, #2`. Jetzt haengt er zusaetzlich am Match-Nibble, an dessen Vergleich
und am `cinc`, also zwei Zyklen mehr auf der einen Kette, an der diese Schleife
nachweislich gebunden ist. Der eingesparte Mispredict ist 3.4 Zyklen auf einem
Drittel der Bloecke, gut ein Zyklus im Mittel; zwei Zyklen auf allen Bloecken
sind teurer.

Eigenes Format unberuehrt, alle elf Zeilen zwischen -1.1% und +1.6% netto --
die Aenderung steht hinter `#ifdef KEVA_LZ4`.

Damit ist die zehnte Idee an dieser Stelle gescheitert, und die dritte in
Folge, die die Kette verlaengert hat, ohne dass es beim Schreiben auffiel. Die
Regel ist inzwischen scharf genug, um sie vorher anzuwenden: **was `x23` fuer
den naechsten Block berechnet, darf nichts Neues beruehren.**

## 2026-09-20 — Der gepolsterte Rest: der Rand kostet nichts mehr

Die erste Idee seit dem memcpy, die haelt, und sie kommt aus demselben Profil
wie die drei davor. `lz4_varied_512` verbrachte 20.5% auf dem geprueften Pfad:

    slow_lit_ready 8.0   slow_mat_ready 4.5   slow_lit_done 3.7
    tail 2.9   slow 1.4

Der Grund ist der Eingabe-Rand. Der schnelle Pfad liest eine feste Strecke ab
dem Cursor -- ein Token und die Literalkopie, siebzehn Bytes -- und hoert so
weit vor dem Ende auf, statt je Block zu pruefen. Ein 512-Byte-Wert von varied
packt auf etwa 250 Bytes in 27 Bloecke, also sind siebzehn Bytes fast zwei
davon, auf einem Pfad, der drei- bis viermal so teuer ist.

### Was gebaut wurde

Wenn der *Eingabe*-Cursor die Zone verlaesst -- und nur dann, der Ausgabe-Rand
ist eine andere Frage -- werden die verbleibenden hoechstens siebzehn Bytes in
vierundsechzig genullte Bytes im eigenen Rahmen kopiert, und `x19` wird um den
Cursor verschoben, sodass jeder Index im Loop seine Bedeutung behaelt. Sonst
weiss nichts im Body davon. Der Rand wird auf das echte Ende gesetzt, und der
schnelle Pfad laeuft bis dorthin.

Dass Polsterung keinen abgeschnittenen Strom verstecken kann, haelt drei
Argumente aus: jeder Block, der ein Byte ab dem Ende liest, laesst den Cursor
hinter dem Ende stehen; ein Offset von null wird dort abgelehnt, wo er gelesen
wird; und `L(done)` prueft, dass der Cursor genau auf dem Ende steht, sonst
uebernimmt der portable Decoder und liefert den Fehler.

### Die Kopie selbst war erst die Haelfte des Gewinns

Byteweise gemessen: geprueter Pfad faellt von 20.5% auf 11.6%, aber
`.Ll_pad_byte` steht mit **6.5%** drin. Siebzehn Bytes einmal je Wert sind auf
einem 512-Byte-Wert kein Nichts. Ersetzt durch eine Breitenleiter -- 16, 8, 4,
2, 1, nur die gesetzten Bits kosten --, also etwa fuenfzehn Instruktionen
statt hundert.

### Gemessen, Mediane aus 3 gepaarten Runden, netto gegen die Kontrollen

| eigenes Format | netto | | fremdes LZ4 | netto |
|---|---|---|---|---|
| records_192 | **+8.1%** | | varied_512 | **+3.3%** |
| noise_64k | +4.0% | | records_4k | +1.4% |
| records_4k | +2.6% | | varied_64k | +1.2% |
| varied_512 | +2.2% | | noise_512 | -2.7% |
| records_512 | +1.1% | | noise_64k | -3.3% |
| varied_4k | -0.7% | | records_64k | -0.4% |

`varied_512` im Fremdformat steht damit bei -0.2% gegen liblz4 statt -3.1%.

Die beiden `noise`-Zeilen sind die einzigen, die sich wehren, und dort feuert
die Polsterung ueberhaupt nicht: ein Literallauf ueber den ganzen Block
verlaesst den schnellen Pfad beim ersten Token, lange bevor der Rand eine
Rolle spielt. In denselben Laeufen lief liblz4 auf genau diesen zwei Zellen um
+2.6% und +2.7% nach oben, weshalb die Netto-Spalte negativ ist. Platzierung,
nicht Mechanik -- der Kernel hat 96 auf 160 Byte Rahmen gewechselt.

x86_64 hat denselben Rand und ist ungemessen.

## 2026-09-20 — Vier Bytes, wo eines gelesen wurde

Die Frage war, ob die knappen Zellen im Fremdformat noch zu holen sind. In
Zyklen je Block gerechnet -- der einzigen Einheit, in der Formen vergleichbar
sind -- war der Rueckstand winzig:

| | B/Block | keva | liblz4 | flex | fehlend |
|---|---|---|---|---|---|
| records_4k | 36.9 | 9.80 | 9.73 | 11.22 | +0.07 |
| records_64k | 32.5 | 9.60 | 9.47 | 9.03 | +0.56 |
| varied_4k | 17.5 | 8.78 | 8.40 | 8.46 | +0.37 |
| varied_512 | 18.9 | 7.21 | 7.19 | 7.98 | +0.02 |

Zwei davon lagen unter dem Rauschboden, also gleichauf. Gesucht waren 0.37 und
0.56 Zyklen.

### Die Aenderung

Der Loop las das Token mit `ldrb` -- ein Byte. Ein `ldr w4` an derselben
Adresse kostet dasselbe und bringt, solange der Literallauf leer ist, auch den
Offset (Bits 8..23) und das Laengenbyte des Matches (Bits 24..31). Was das
ersetzt, sind zwei weitere abhaengige L1-Zugriffe: die Offsetadresse ist
`Token -> shift -> add`, liegt also drei Zyklen hinter dem Token, und das
Laengenbyte nochmal drei dahinter -- auf der Kette, die den Ausgabecursor
speist, auf den der naechste Block wartet. Dazu entfaellt fuer diese Bloecke
die unbedingte 16-Byte-Literalkopie, die ohnehin ueberschrieben wurde.

Der Zensus sagt, wie oft: Literallauf leer in 88% von records_64k, 56% von
varied_4k, einem Drittel des Rests. Bei records_64k saettigen davon 75% auch
das Match-Nibble, also nimmt zwei Dritteln seiner Bloecke beides aus diesem
einen Load.

Der befuerchtete Haken -- ein Muenzwurf-Sprung auf "Literallauf leer" bei
varied -- ist nicht eingetreten. Der haeufige Fall faellt durch statt zu
springen, und 56/44 reicht dem Praediktor offenbar.

### Gemessen, Mediane aus 3 gepaarten Runden, netto gegen die Kontrollen

| fremdes LZ4 | alt | neu | netto | jetzt vs liblz4 |
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

**17 von 18 Zellen** im Fremdformat, gegen 10 vorher. Die eine Ausnahme ist
noise_4k mit -0.3%, also Paritaet. Das eigene Format steht unveraendert bei 21
von 22 -- die Aenderung liegt hinter `#ifdef KEVA_LZ4` und alle elf Zeilen
bewegen sich zwischen -2.3% und +1.2% netto.

Korrektheit: 105 Tests mit `liblz4`, die 5760 Faelle des Soaks, und alle neun
Benchmark-Formen byte-fuer-byte gegen das, was liblz4 gepackt hat.

### Was das ueber den Rest des Tages sagt

Elf Ideen, neun davon null oder schlechter. Die drei, die getragen haben, haben
dasselbe getan: **Arbeit entfernt, nicht Instruktionen.** memcpy nahm eine
Schleife weg (+34%), die Polsterung nahm den geprueften Pfad weg (+8%), und
diese hier nimmt zwei abhaengige Loads und einen Vektor-Store weg (+37%). Alles,
was stattdessen die Kette umbauen oder Befehle sparen wollte, hat zwischen 0%
und -31% gekostet.

## 2026-09-20 — Derselbe Trick im even-Split

Nach dem LZ4-Body las unser eigenes Format `varied_4k` mit 8.28 GiB/s, waehrend
derselbe Kernel einen *fremden* LZ4-Block derselben Daten mit 8.63 las. Der
Grund war nur die Wache: `varied` landet bei uns im even-Split, der dieselbe
Token-Aufteilung hat wie LZ4 und den Trick nicht bekommen hatte. `#ifdef
KEVA_LZ4` wurde zu `#ifndef KEVA_WIDE`.

Mediane aus drei gepaarten Runden, netto gegen die Kontrollen:

| eigenes Format | alt | neu | netto | jetzt vs liblz4 |
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

22 von 22 Zellen bleiben gewonnen.

### Die zwei Minus sind echt, und sie haben einen Grund

Ueber je fuenf gepaarte Laeufe bestaetigt, Kontrolle flach bei +0.2%:
records_192 -3.0%, records_512 -1.5%.

Der Zensus sagt warum. Der schnelle Pfad faellt in den Sonderfall hinein statt
zu ihm zu springen, weil ein leerer Literallauf 88% von records_64k in LZ4s
Format ausmacht und 56% von varied. In *unserem* Format traegt records aber in
56% seiner Bloecke einen bis drei Literale und nur in 32% keinen -- dort ist
die Reihenfolge verkehrt herum.

### Umgedreht gebaut, und es ist schlimmer

Den Sonderfall aus dem Fallweg genommen und per Sprung erreicht:

| | netto |
|---|---|
| eigenes records_192 | -3.0% -> -2.1% |
| fremdes records_64k | **-7.9%** |
| fremdes varied_512 | **-11.8%** |
| fremdes noise_512 | **-11.6%** |

Eine Anordnung kann nicht beiden Verteilungen dienen. Die Zahlen waehlen die,
die steht.

### Und der Grund, warum es nicht feiner geht

Naheliegend waere, den Sonderfall auf Literallaeufe bis drei auszuweiten --
das deckt 88% von records und 76% von varied, beide als Durchfall. Er ist
durchgerechnet und nicht gebaut: der Offset saesse dann an einer variablen
Position, also `ubfx` auf die Laenge, Schieben, Maskieren, und der Offset
landet acht Zyklen hinter dem Token statt vier. Genau die vier Zyklen sind der
ganze Gewinn. Die Verallgemeinerung wuerde das wegwerfen, was sie
verallgemeinert.

## 2026-09-20 — Das Repeat-Bit zurueck in den even-Split? Gezaehlt, nicht gebaut

`records_512` war einmal 180 B und ist heute 195. Die fuenfzehn Bytes sind
gegangen, als das Repeat-Bit aus dem even-Split in den wide-Split gezogen ist,
weil es dort 22.9% Dekodierzeit kostete. Die Frage war, ob der Null-Literal-Pfad
von heute das aufwiegt.

Er kann es nicht, und das ist eine Mengenbetrachtung: der neue Pfad
beschleunigt Bloecke mit Literallauf **null**, der Preis von 3/4/1 trifft
Bloecke mit Literallauf **ab sieben**. Disjunkt.

Der Zensus ueber unsere eigenen gepackten Werte (`examples/even_census.rs`):

| | Bloecke | Kette heute | Kette dann | neu | Anteil | Repeats | Groesse |
|---|---|---|---|---|---|---|---|
| records_192 | 4 | 2 | 2 | 0 | 0% | 1 | -2 B (-1.5%) |
| records_512 | 12 | 2 | 5 | 3 | 25% | 9 | **-15 B (-7.7%)** |
| varied_512 | 19 | 6 | 12 | 6 | 32% | 11 | -16 B (-4.5%) |
| varied_4k | 186 | 50 | 113 | 63 | 34% | 35 | -7 B (-0.3%) |
| varied_64k | 3054 | 738 | 1701 | 963 | 32% | 171 | **+621 B (+1.9%)** |

Die 195 -> 180 stimmen auf das Byte mit dem Zensus vom 18.9. ueberein, was die
Zaehlung validiert.

**Zwei Gruende, es nicht zu bauen.** Ein Drittel aller Bloecke wuerde neu aus
der Schleife heraus in die Literalkette springen -- genau der Mechanismus, der
-22.9% und -21.5% gemessen hat. Und die Groesse traegt es nicht: der Gewinn
existiert nur bei 512 Bytes, faellt bei 4 KiB auf 0.3% und dreht sich bei
64 KiB um, weil die Wiederholungsrate mit der Groesse faellt (5.6% bei
varied_64k) und die Literalkette mit ihr steigt.

Derselbe Handel wie am 18.9., diesmal vorher ausgerechnet statt hinterher
gemessen. Das ist der Punkt, an dem ein Zensus eine Messung ersetzt.

## 2026-09-20 — x86 bekommt den breiten Token-Load, und ein Test, der vorher nichts geprueft hat

Der Packer laeuft auf x86, die Decoder haben von den vier Gewinnen des Tages
keinen. Portiert ist zunaechst nur einer -- der breite Token-Load, auf aarch64
zwischen +22% und +52% wert -- und zwar allein, weil jede Antwort auf x86 eine
gemietete Maschine kostet: zwei Aenderungen in einer Runde waeren bei einem
schlechten Ergebnis nicht auseinanderzuhalten.

Die x86-Stelle ist dieselbe: `movzwl (%rsi,%r10,1), %ecx` laedt den Offset an
einer Adresse, die aus dem Token kommt, also Token laden, schieben, Offset
laden. `mov (%r14), %eax` statt `movzbl` bringt Token, Offset und
Laengenbyte in einem.

### Der Test, der grün war und nichts geprueft hat

Geschwindigkeit braucht die Cloud, Korrektheit nicht -- beide Ziele bauen und
testen hier. Nur: der vorhandene Ueberlapp-Test setzt seine Literalzahl aus dem
Offset und erreicht damit nie null, also war der neue Pfad ungedeckt. Der neu
geschriebene Test war es beim ersten Versuch ebenfalls:

    DIAG entschieden=0 abgelehnt=208

Null von 208 handgebauten Bloecken wurden ueberhaupt dekodiert -- der erste
Token trug ein Match-Nibble, aber ich hatte keinen Offset dahintergeschrieben,
und `continue` bei Ablehnung hat das verschluckt. Ein gruener Test, der nichts
ausfuehrt.

Zwei Dinge haben das repariert. Der Block ist jetzt gueltig aufgebaut (Seed-Lauf
mit Match, dann drei Bloecke ohne Literale, dann ein Literalschwanz), und die
Zusicherung ist `refused == 0` statt `decided > refused`: eine Ablehnung ist
die legitime Antwort des Kernels, aber sie ist auch, wie ein kaputter
Schnellpfad sich versteckt -- er verschiebt den Rahmen, der naechste Block
faellt durch eine Wache, und ein Test, der nur Bytes vergleicht, bleibt gruen.

### Gegengeprueft durch Mutation

| Mutation | aarch64 | x86 |
|---|---|---|
| Cursor-Schritt 3 -> 2 | FAILED | FAILED |
| Offset aus Bit 8 -> 9 | FAILED | -- |
| Erweiterungsbyte Bit 24 -> 25 | -- | FAILED |

Erst damit ist belegt, dass der Test den Pfad ausfuehrt. Ohne diese Probe haette
ich einen ungetesteten Assemblerpfad auf eine Architektur geschoben, die ich
nicht messen kann.

Stand: 106 Tests auf aarch64, 102 auf x86, 5760 Soak-Faelle, clippy still.
Ungemessen bleibt die Geschwindigkeit auf x86 -- dafuer braucht es eine Runde
auf Xeon und EPYC.

## 2026-09-20 — liblz4 fuer x86 selbst gebaut, und was es sofort gefunden hat

Der Soak mit 5760 von liblz4 gepackten Werten lief nur auf aarch64, weil
Homebrew hier kein x86-liblz4 hat. Der x86-Kernel war damit durch etwa
zweihundert handgebaute Bloecke gedeckt und durch nichts sonst. lz4 hat keine
Abhaengigkeiten und baut in vier Sekunden:

    clang -arch x86_64 -O2 -c lz4.c lz4hc.c lz4frame.c xxhash.c
    ar rcs liblz4.a *.o

Unter Rosetta gemessen waere sinnlos; ausgefuehrt ist es exakt derselbe
Assembler, und Korrektheit ist genau das, was Rosetta nicht veraendert.

Erster Lauf gegen den frisch portierten x86-Kernel:

    5760 Faelle, 9 falsch

Alle neun **REFUSED**, keiner WRONG -- der Kernel lehnt ab, der portable
Decoder uebernimmt, die Ausgabe stimmt. Deshalb blieb jeder Test gruen. Gegen
`HEAD~1` gemessen: 0 falsch. Also von mir eingebaut, in derselben Stunde.

### Halbiert statt geraten

Die Abkuerzung fuer literallose Bloecke deaktiviert, den breiten Load behalten:
**immer noch neun**. Damit war es nicht die Abkuerzung, sondern der Load selbst.

`L(dst_edge)` ist der eine Ausgang aus dem Schleifenkopf, der das Token in
`%eax` *weiterreicht*, statt es neu zu laden -- `L(src_edge)` und
`L(fast_litend)` machen beide ihr eigenes `movzbl`, und im Quelltext steht
sogar, warum. Mit einem Dword-Load trug dieser Pfad drei Bytes des naechsten
Blocks in `L(slow)`, das sie als Literalzahl schob.

Ein `movzbl %al, %eax` auf einem Pfad, der einmal je Wert laeuft: 0 von 5760.

### Was das ueber den Tag sagt

Zwei Fehler heute, beide derselben Art: ein `#ifdef`-Zweig ist ein zweites
Programm, und beide Male hat das zweite Programm eine Annahme gebrochen, die
im ersten stillschweigend galt. Der Absturz heute frueh (Tabellen vor dem
`ret`) und dieser hier. Beide gefunden, nicht weil ein Test sie vorhersah,
sondern weil eine *unabhaengige* Referenz Byte fuer Byte verglichen hat.

## 2026-09-21 — Die AVX-Schwelle: gebaut, gemessen, widerlegt

`noise_512` im Fremdformat liegt auf Xeon 30% hinter liblz4. Das Profil schien
den Grund zu nennen -- `perf annotate` legte 78.9% des Kernels auf zwei
Instruktionen der 64-Byte-Literalschleife, den *zweiten* `vmovdqu`-Load und den
*zweiten* Store, waehrend das erste Paar derselben Iteration 0.00% zog. Dazu
passte die Groessenreihe: noise_64k +4.4% (1024 Durchlaeufe), noise_4k +4.1%
(64), noise_512 -31% (8). Ein Aufwand, der bei acht Durchlaeufen dominiert und
bei vierundsechzig verschwindet, liest sich wie eine Anlaufkosten.

Also eine Schwelle: Laeufe unter einem Kilobyte durch die 16-Byte-Schleife,
laengere durch die 256-Bit-Schleife. Gemessen in beiden Reihenfolgen, weil die
512-Byte-Zellen ueber den Abend um dreissig Punkte geschwankt haben:

| noise_512, GiB/s | Position 1 | Position 2 |
|---|---|---|
| ohne Schwelle | 35.23 | 35.20 |
| mit Schwelle | 24.31 | **23.75** |

Positionsunabhaengig und eindeutig: die breite Schleife ist auch bei acht
Durchlaeufen um 32% besser. Zurueckgenommen.

`records_512` zeigte in einer Position -10.2% und in der anderen +16.4% --
dieselbe Revision. Die Zelle ist ohne beide Reihenfolgen nicht lesbar, was der
Skriptkopf seit Tagen sagt und was hier zum zweiten Mal fast zu einem falschen
Urteil gefuehrt haette.

### Was bleibt

Bei 512 Bytes sind es rund 36 Zyklen gegen liblz4s 25, und die Kopie selbst ist
davon nur etwa zehn. Der Rest sind Kosten, die einmal je Wert anfallen: zwoelf
Push/Pop, der Zonenaufbau, und vor allem die Literallaengen-Kette -- 512 Bytes
brauchen zwei Erweiterungsbytes, zwei abhaengige L1-Zugriffe hintereinander, auf
einem Wert, der aus genau einem Block besteht und deshalb nichts hat, womit er
diese Latenz ueberlappen koennte.

### Und eine Testluecke, die schwerer wiegt

Der AVX-Store wurde absichtlich verfaelscht -- der Soak blieb gruen.
`Lz4Body::all()` gibt auf einer CPU ohne AVX2 nur `[Baseline, Ssse3]` zurueck,
und unter Rosetta gibt es kein AVX2. Die Xeon- und EPYC-Bodies laufen auf diesem
Rechner also **nie**, weder im Soak noch in den Tests. Auf der gemieteten
Maschine nachgeholt: dieselbe Mutation laesst dort den Soak und zwei Tests
fallen. Diese beiden Bodies sind ausschliesslich in der Cloud pruefbar.

## 2026-09-21 — Der letzte Literallauf, ohne zweimal zu fragen

Gefunden durch Lesen, nicht durch Messen. `L(fast_litext)` liest die
Literallaengen-Kette, stellt dann fest, dass der Lauf bis ans Eingabeende reicht
-- `lea 2(%rsi,%r10,1) / cmp %r13 / ja L(fast_litend)` -- und springt nach
`L(slow)`, wo das Token **neu geladen**, die Nibbles **neu extrahiert** und
**dieselbe Kette ein zweites Mal** gelaufen wird.

Bei einem Wert, der aus genau einem Block besteht, liegt das vollstaendig auf
dem kritischen Pfad. 512 Bytes inkompressibler Daten sind genau das: Token,
zwei Erweiterungsbytes, 512 Literale. Vier abhaengige L1-Zugriffe, wo zwei
genuegen.

Am Abbruchpunkt ist alles schon da: `rsi` hinter den Laengenbytes, `rdi` am
Ausgang, `r10` die Laenge. Es fehlten die zwei Schranken, die der schnelle Pfad
noch nicht geprueft hatte, und `r14` muss auf den Lauf statt aufs Token zeigen,
weil `L(lit_done)` es um die Lauflaenge weiterschiebt. Acht Instruktionen statt
etwa fuenfunddreissig und drei abhaengigen Loads.

### Gemessen, Xeon 8481C, vier Positionen in beiden Reihenfolgen

Position 1 ist verworfen: dieselbe Revision las dort `noise_512` mit 15.29 und
in Position 3 mit 35.14 GiB/s. Der Kaltstart trifft, was zuerst gemessen wird,
und der Skriptkopf sagt das seit Tagen.

| | alt | neu | delta | vs liblz4 vorher -> jetzt |
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

Sieben von neun vor liblz4. Die Aenderung kostet nirgends mehr als ein Prozent
und ist auf genau den Formen gross, die aus wenigen langen Literallaeufen
bestehen.

### Was an noise_512 bleibt

40.68 GiB/s sind 31 Zyklen fuer 512 Bytes, liblz4 braucht 25. Sechs Zyklen, und
die Kopie selbst ist davon keine -- acht Durchlaeufe zu je 64 Byte sind etwa
zehn. Was bleibt, sind Rahmen und Zonenaufbau, also Kosten, die ein Wert aus
einem einzigen Block nicht auf mehrere Bloecke verteilen kann.

aarch64 hat dieselbe Doppelung und ist ungemessen.

### Korrektur zum Eintrag darueber: records_512 zeigt nichts

Die +24.7% auf records_512 sind kein Ergebnis. Ueber alle Laeufe dieses Tages:

    8746808 (alt)  8.06  8.50  6.78  6.60
    023e1c5 (neu)  8.06  8.39

Beide Revisionen liegen im selben Band von 6.6 bis 8.5 GiB/s, eine Spanne von
29%. Dass in dem einen Lauf die alte Revision unten und die neue oben lag, ist
die Zelle und nicht die Aenderung. Der Eintrag darueber liest +24.7%, und das
ist falsch.

Belastbar aus demselben Lauf ist nur, was sich ueber mehrere Messungen
wiederholt:

    noise_512   alt  35.23  35.20  35.14     neu  40.47  40.90   +15.6%
    noise_64k   alt  34.16                   neu  35.17  35.59    +3.6%
    noise_4k    alt  67.77  67.77            neu  70.61  67.77    +2.1%

noise_512 ist damit von -31% auf -20% gegen liblz4, und das haelt. Die anderen
sechs Zellen bewegen sich nicht.

Dies ist das dritte Mal heute, dass eine 512-Byte-Zelle beinahe ein falsches
Urteil gekauft hat. Fuer diese Groesse gilt: mindestens zwei Messungen je
Revision, und wenn die Baender sich ueberlappen, ist das Ergebnis "nichts" und
nicht der Mittelwert.

## 2026-09-22 — Die Polsterung auf x86: widerlegt, und eine falsche Praemisse dazu

Die Frage war, wieso der Xeon-Kernel viele Zyklen braucht. Er braucht keine.
Zyklen je Block, aus Durchsatz und Blockzahl gerechnet:

| | Bloecke | Xeon | M2 |
|---|---|---|---|
| records_64k | 1567 | **8.9** | 8.7 |
| records_4k | 96 | **9.2** | 8.7 |
| records_512 | 12 | 11.8 | 10.4 |
| records_192 | 4 | 36.9 | 16.2 |

Bei 1567 Bloecken liegt x86 zwei Prozent neben Apple Silicon, bei identischem
Quelltext. Die Schleife ist nicht das Problem.

Der Ausreisser bei records_192 war eine einzelne Messung, und das Modell ging
nicht auf: 148 Zyklen fuer vier Bloecke gegen 141 fuer zwoelf ist weniger Zeit
fuer dreimal so viel Arbeit. Eine feste Grundlast plus Arbeit je Block kann das
nicht erzeugen. Vier Positionen spaeter liest records_192 3.26 und 3.22 GiB/s --
stabil, und der Ausreisser war die Messung.

### Gebaut und zurueckgenommen

Die Polsterung des Eingaberests, auf aarch64 +8.1% auf genau dieser Form, nach
x86 portiert. Vier Positionen, warm gegen warm:

| | alt | neu | delta |
|---|---|---|---|
| same_bytes records_512 | 8.00 | 7.35 | **-8.1%** |
| own_format records_512 | 9.32 | 8.65 | **-7.2%** |
| same_bytes noise_512 | 45.99 | 43.18 | -6.1% |
| own_format varied_4k | 7.00 | 6.75 | -3.6% |
| same_bytes noise_64k | 35.39 | 34.25 | -3.2% |
| own_format records_192 | 3.26 | 3.22 | -1.2% |

Die Zelle, fuer die sie gebaut wurde, bewegt sich nicht, und records_512
verliert acht Prozent. Zurueckgenommen.

**Ein Verdacht, der nicht aufgeklaert ist.** own_format records_32 las 1.40 und
1.18 GiB/s -- sauber getrennt nach Revision -- obwohl diese Zelle den Kernel
ueberhaupt nicht anfasst: bei 32 Bytes weigert sich der Packer, der Wert wird
roh gespeichert, und der Benchmark macht dort ein `copy_from_slice`. Eine
Aenderung am Assembler kann diese Zahl nicht bewegen. Sie bewegt sich trotzdem,
also verschiebt die geaenderte Objektgroesse etwas im Binary. Damit ist ein Teil
der -7% und -8% oben moeglicherweise Platzierung und nicht Mechanik -- fuer das
Urteil egal, weil die Aenderung so oder so nichts eintraegt, aber es heisst,
dass Deltas dieser Groesse hier nicht allein der Logik zuzuschreiben sind.

### Wo die beiden Kernel inzwischen auseinanderlaufen

Gleich: zwei Token-Aufteilungen als getrennte Bodies, eigener LZ4-Body,
Zonenschleife plus geprueft, Shuffle-Tabellen fuer kurze Ueberlappungen,
COPY_MAX-Leiter, memcpy fuer den letzten Literallauf.

Verschieden, jedes Mal gemessen:

| | aarch64 | x86_64 |
|---|---|---|
| Weiche fuer literallose Bloecke | `cbnz w5`, nur das Literal-Nibble | `cmp $0x0F`, das ganze Token |
| memcpy-Schwelle | ab 8192 B | ab 128 B |
| Matchschleife | zwei Formen | eine |
| Teilelinien | keine | Xeon, EPYC, SSSE3 |
| Eingaberest gepolstert | ja (+8.1%) | nein (-8.1%) |

Die ersten beiden widersprechen sich offen. Der Test aufs Literal-Nibble war auf
ARM ein Gewinn und hat auf Xeon records_512 neun Prozent gekostet, weil er dort
ein Muenzwurf ist. Und memcpy war auf ARM bei 512 Bytes 10.4% schlechter als die
eigene Schleife, auf Xeon 14.2% besser -- Apples memcpy verliert dort, glibcs
gewinnt. Vier Ideen wurden heute von einer Plattform auf die andere portiert,
und zwei davon haben das Vorzeichen gewechselt.

## 2026-09-22 — Vier Zellen, die nie einen Decoder gemessen haben

Die Frage war, wieso records_32 im eigenen Format auf Xeon 50% hinten liegt,
wo unser Decoder dort gar nicht laufen kann. Er lief auch nicht.

Bei 32 Bytes lehnt der Packer ab -- `worth_storing` verlangt acht gesparte
Bytes und 12.5%, LZ4 holt aus 32 Byte JSON zwei -- und der Wert wird roh
gespeichert. Der Benchmark hat dafuer

    mine[..data.len()].copy_from_slice(&data);

gemessen und das "den Lesepfad" genannt. Der Store tut das nicht:

    if flags.compressed { pack::unpack(...); Cow::Owned(out) }
    else                { Cow::Borrowed(bytes) }

Ein roh gespeicherter Wert wird **ausgeliehen, nicht kopiert** -- ein
Arena-Slice, null Bytes. Die Zelle hat also eine Kopie gegen einen echten
Decode gestellt, die in Wirklichkeit nicht stattfindet.

Dass die Zahl absurd war, stand in ihr selbst: 1.40 GiB/s fuer 32 Bytes sind
57 Zyklen, und dieselbe Zeile misst auf M2 17.5. Identischer Rust, Vorzeichen
gedreht -- +87% dort, -50% hier.

**Und es betrifft nicht nur die schlechte Zelle.** Der Packer lehnt vier Formen
ab: records_32, noise_512, noise_4k, noise_64k. Alle vier haben in `own_format`
ein memcpy gemessen, und **drei davon haben wir gross "gewonnen"**: +76.6%,
+34.2%, +3.5% auf Xeon. Von den bisher gemeldeten 22 Zellen waren acht keine
Decoder-Vergleiche.

Der Benchmark ueberspringt diese Formen jetzt und sagt es:

    own_format records_32: packer refused, stored raw -- no decode to time

Sie bleiben in `pack/sizes` und `compress3`, wo "roh gespeichert" das Ergebnis
ist und kein Artefakt.

### Eigenes Format danach, M2 Max, Mediane aus drei

| | keva | liblz4 | flex | vs liblz4 | vs flex |
|---|---|---|---|---|---|
| varied_64k | 9.72 | 5.08 | 7.83 | **+91.3%** | +24.1% |
| records_192 | 9.78 | 6.34 | 7.70 | +54.3% | +27.0% |
| records_64k | 15.87 | 11.23 | 11.60 | +41.4% | +36.8% |
| varied_4k | 9.71 | 6.87 | 7.08 | +41.3% | +37.1% |
| records_4k | 16.02 | 12.52 | 11.11 | +28.0% | +44.3% |
| varied_512 | 10.31 | 8.75 | 9.11 | +17.8% | +13.1% |
| records_512 | 13.01 | 11.12 | 10.76 | +17.0% | +21.0% |

14 von 14, und jede davon ist ein Decode gegen einen Decode.

## 2026-09-22 — Der letzte Literallauf byteweise: +119.5% auf records_192

Die Zelle hatte ich zweimal als unzuverlaessige Messung abgetan. Sie war stabil
und hatte recht.

Ein Kostenmodell aus records_512 und records_4k, je Plattform gefittet:

| | Zyklen je Block | fest je Aufruf | records_192 vorhergesagt | gemessen |
|---|---|---|---|---|
| Xeon | 8.9 | 31 | 67 | **148** (2.22x) |
| M2 | 8.4 | 28 | 61 | 64 (1.05x) |

Dieselbe Form passt auf ARM ins Modell und kostet auf x86 das Doppelte. Das
Profil zeigt in einer Zeile, warum:

    26.39%  movzbl (%r9),%r14d
    10.87%  inc    %r9
     6.34%  inc    %rdx
     6.08%  dec    %rax
     5.95%  mov    %r14b,(%rdx)

55% des Kernels in einer Byte-fuer-Byte-Kopie. Erreicht wird sie, sobald ein
Literallauf weniger als zweiunddreissig Bytes Luft bis zum Eingabeende hat --
denn ein Block dieser Breite wuerde darueber hinauslesen. Eine Leiter tut das
nicht, und eine steht seit jeher ein paar hundert Zeilen weiter in derselben
Datei.

records_192 packt auf 134 Bytes in vier Bloecken, also kommt jeder Lauf, der
hinter Byte 102 endet, dort an: die halbe Datei. records_512 packt auf 195 in
zwoelf, und nur der letzte tut es. Daher 32 gegen 10 Zyklen je Block.

### Gemessen, Xeon, vier Positionen

| unser Format | alt | neu | delta |
|---|---|---|---|
| records_192 | 3.27 | 7.17 | **+119.5%** |
| varied_512 | 7.48 | 8.16 | +9.1% |
| records_512 | 9.22 | 9.45 | +2.5% |
| varied_4k | 7.01 | 7.17 | +2.3% |
| records_4k | 11.64 | 11.79 | +1.3% |
| records_64k | 11.93 | 11.83 | -0.8% |

| Standard LZ4 | alt | neu | delta |
|---|---|---|---|
| varied_512 | 6.03 | 6.31 | +4.6% |
| records_4k | 11.03 | 11.12 | +0.8% |
| noise_512 | 45.93 | 44.94 | **-2.2%** |

7.17 gegen 7.17 in beiden neuen Positionen. records_192 geht damit von -23.0%
auf **+69.1%** gegen liblz4.

Stand auf Xeon danach: unser Format **14 von 14**, Standard LZ4 **16 von 18**.

### Was der Fund ueber den Tag sagt

Die Zelle wurde zweimal falsch eingeordnet, beide Male von mir, beide Male mit
einem Argument statt einer Messung: erst "512-Byte-Zellen schwanken", dann "der
Benchmark misst den Messaufbau". Was sie aufgeklaert hat, war ein Kostenmodell
aus zwei anderen Zellen -- 148 Zyklen fuer vier Bloecke gegen 138 fuer zwoelf
ist arithmetisch unmoeglich, und das war schon sichtbar, bevor irgendeine
Maschine lief.

## 2026-09-23 — Der Schnellausgang im memcpy-Zweig: widerlegt

Wo der Literallauf den Wert abschliesst, braucht der Ruecksprung keine
geretteten Register: acht statt vierundzwanzig Byte Stack, vier
Speicherzugriffe weniger. Gemessen auf Xeon, vier Positionen, `noise`:

| | P1 alt | P2 neu | P3 alt | P4 neu | alt -> neu |
|---|---|---|---|---|---|
| noise_512 | 45.28 | 43.42 | 45.17 | 43.27 | **-4.2%** |
| noise_4k | 66.88 | 66.38 | 66.81 | 66.41 | -0.7% |
| noise_64k | 36.59 | 34.04 | 34.87 | 34.12 | -3.6% |

Sauber nach Revision getrennt. Das `lea`/`cmp`/`jne` vor jedem Aufruf kostet
mehr als die zwei Stores und zwei Loads, die es spart, und der zusaetzliche
Verzweigungspunkt zieht den Code auseinander. Zurueckgenommen.

Damit bleibt noise_512 auf Xeon bei -11.9% gegen liblz4, und der Rueckstand ist
weiterhin mit 3.4 Zyklen auf 28.7 beziffert. Auf M2 gewinnen wir dieselbe Zelle
mit 34.3 gegen 34.7 Zyklen -- der Unterschied ist, dass glibcs memcpy auf x86
27% schneller wird als auf ARM und unsere Fassung nur 16%.

### Zum Ablauf, weil er Geld gekostet hat

Der vorherige Anlauf dieser Messung ist um 00:25 haengen geblieben und wurde um
09:49 gefunden: neun Stunden Instanzzeit. Jeder Statuscheck dazwischen hat
gefragt, ob der Prozess lebt, statt wann die Logdatei zuletzt geschrieben
wurde. Der Neuanlauf hat deshalb drei Bremsen -- eine Notabschaltung nach
vierzig Minuten im Skript selbst, `timeout 300` je Messzelle, und einen
Waechter auf den Zeitstempel der Logdatei.

## Alle sechzehn Entpackformen im Profil, auf EPYC und auf Axion

`perf record` je Form, Symbol aus `perf report` gelesen statt geraten, dazu die
zehn heissesten Instruktionen aus `perf annotate`. EPYC 9B14 (c3d-standard-4,
europe-west4-a), Axion / Neoverse V2 (c4a-standard-4, europe-west4-c). Beide
Maschinen je fuer diesen einen Lauf erzeugt und danach geloescht.

Der erste Anlauf auf Axion hat gar nichts geliefert: c4a nimmt `pd-balanced`
nicht und braucht `hyperdisk-balanced`, und das Image muss `debian-12-arm64`
sein. Das Bench-Skript weiss beides, das Profilskript wusste es nicht.

### Axion, eigenes Format

| Form | Kernel | heisseste Instruktion | Anteil | was sie ist |
|---|---|---|---|---|
| records_192 | 89.0% | `str q0, [x21, x24]` | 20.6% | die unbedingte 16-B-Literalkopie |
| records_512 | 96.5% | `stp q0, q1, [x13], #32` | 14.0% | die Match-Kopie |
| varied_512 | 94.6% | `stp q0, q1, [x13]` | **39.0%** | die Match-Kopie |
| records_4k | 77.2% wide | `stp q0, q1, [x13]` | 20.2% | die Match-Kopie |
| varied_4k | 99.4% | `stp q0, q1, [x13]` | **36.5%** | die Match-Kopie |
| records_64k | 97.9% wide | `stp q0, q1, [x13]` | 27.4% | die Match-Kopie |
| varied_64k | 99.0% | `stp q0, q1, [x13]` | **38.7%** | die Match-Kopie |

### Axion, Fremdformat

| Form | Kernel | heisseste Instruktion | Anteil | was sie ist |
|---|---|---|---|---|
| lz4_records_512 | 98.3% | `stp q0, q1, [x13]` | 20.4% | Match-Kopie |
| lz4_varied_512 | 98.5% | `stp q0, q1, [x13]` | **43.1%** | Match-Kopie |
| lz4_records_4k | 98.3% | `stp q0, q1, [x13]` | 26.2% | Match-Kopie |
| lz4_varied_4k | 99.8% | `stp q0, q1, [x13]` | **43.8%** | Match-Kopie |
| lz4_records_64k | 98.6% | `stp q0, q1, [x13]` | 17.5% | Match-Kopie |
| lz4_varied_64k | 99.6% | `stp q0, q1, [x13]` | **45.7%** | Match-Kopie |
| lz4_noise_512 | 91.8% | `stp q0, q1, [x0], #32` | 45.6% | **die eigene 32-B-Literalschleife** |
| lz4_noise_4k | 98.5% | `stp q0, q1, [x0], #32` | 57.7% | **die eigene 32-B-Literalschleife** |
| lz4_noise_64k | 14.9% | `0x9d3f8` u.a., 60% zusammen | | **glibc memcpy** |

### EPYC, eigenes Format

| Form | Kernel | heisseste Instruktion | Anteil | was sie ist |
|---|---|---|---|---|
| records_192 | 77.9% | `shl $0x4,%ecx` | **23.4%** | Index in die Splat-Tabelle |
| records_512 | 86.7% | `movdqu %xmm0,(%r8)` | 14.5% | Literalkopie; `shl $4` 11.8% |
| varied_512 | 89.5% | `movdqu 0x10(%rsi),%xmm1` | 15.0% | Literalkopie |
| records_4k | 82.3% wide | `add $0x20,%rsi` | 10.8% | Match-Kopie |
| varied_4k | 98.6% | `movdqu %xmm1,0x10(%r8)` | 18.2% | Literalkopie |
| records_64k | 97.8% wide | `add $0x20,%rsi` | 7.6% | Match-Kopie |
| varied_64k | 98.2% | `movdqu 0x10(%rsi),%xmm1` | 16.5% | Literalkopie |

### EPYC, Fremdformat

| Form | Kernel | heisseste Instruktion | Anteil | was sie ist |
|---|---|---|---|---|
| lz4_records_512 | 96.2% | `shl $0x4,%ecx` | **25.4%** | Index in die Splat-Tabelle |
| lz4_varied_512 | 98.2% | `movdqu 0x10(%rsi),%xmm1` | 9.7% | Literalkopie; `shl $4` 8.5% |
| lz4_records_4k | 98.0% | `movdqu 0x10(%rsi),%xmm1` | 9.9% | Literalkopie |
| lz4_varied_4k | 99.3% | `movdqu %xmm1,0x10(%r8)` | 19.5% | Literalkopie |
| lz4_records_64k | 98.4% | `movdqu 0x10(%rsi),%xmm1` | 29.2% | Literalkopie |
| lz4_varied_64k | 98.9% | `movdqu 0x10(%rsi),%xmm1` | 27.2% | Literalkopie |
| lz4_noise_512 | 33.3% | `pop %rbp` 18.5%, `push %rbp` 11.1% | | **Prolog und Epilog** |
| lz4_noise_4k | 2.0% | `0x16db20` | **98.1%** | **glibc memcpy** |
| lz4_noise_64k | 8.1% | `0x16db75` | **89.4%** | **glibc memcpy** |

### Was das Profil beweist

Der unbenannte Posten taucht auf beiden Maschinen genau dort auf, wo die
jeweilige memcpy-Schwelle faellt -- auf EPYC ab 4k (`MEMCPY_MIN 128`), auf Axion
erst bei 64k (`LIT_MEMCPY 8192`). Das identifiziert ihn: es ist glibc memcpy,
dem nur die Symbole fehlen. Die beiden Maschinen bestaetigen sich gegenseitig.

Damit sind alle `noise`-Zellen auf beiden Maschinen dieselbe Entscheidung, und
sie faellt in **entgegengesetzte Richtungen**:

- **Axion, noise_4k -33.1% und noise_512 -16.7%:** wir kopieren dort mit der
  eigenen 32-Byte-Schleife, weil `LIT_MEMCPY 8192` noch nicht greift. Die
  Schleife kostet `ldp`, `stp`, `subs`, `cmp`, `cbz` je 32 Bytes; glibc auf
  Neoverse V2 laeuft 64 Bytes je Durchgang ohne Vergleichskette. Bei 64k, wo
  memcpy greift, ist der Rueckstand weg. Die Schwelle 8192 ist auf dem M2 Max
  gemessen und fuer Neoverse V2 zu hoch.
- **EPYC, noise_4k -7.9%:** dort ist bereits glibc memcpy zu 98% drin, und
  glibc verliert gegen die Wildcopy in liblz4, die ueberlaufen darf und keinen
  exakten Rest behandelt. **noise_512 -22.4%** ist gar keine Kopie: Prolog und
  Epilog sind 30% der Kernelzeit, dazu der memcpy-Aufruf. Bei 512 Bytes ist der
  feste Aufwand die Messung. `MEMCPY_MIN 128` ist auf Xeon gemessen und fuer
  Zen 4 zu niedrig.

Das trifft sich guenstig: x86 hat eine eigene Teilelinie je Kern, `MEMCPY_MIN`
laesst sich fuer EPYC anheben, ohne Xeon anzufassen. **aarch64 hat keine
Teilelinien** -- ein Body je Split, geteilt zwischen M2 Max und Neoverse V2. Eine
niedrigere `LIT_MEMCPY` trifft beide. Der erste Schritt ist deshalb eine
Schwellenreihe lokal auf dem M2 Max, die nichts kostet: ist der M2 zwischen 512
und 8192 gleichgueltig, faellt die Schwelle global und Axion bekommt seine 33%
umsonst. Braucht der M2 die 8192, dann kostet es eine Teilelinie auf aarch64.

Der zweite Posten, den nur EPYC hat: `shl $0x4,%ecx` mit 23.4% bei records_192
und 25.4% bei lz4_records_512, dazu das `lea` auf die Tabelle. Das ist der
Splat fuer Matches mit Offset unter 16 -- ein von der Offsetgroesse abhaengiger
Ladevorgang aus einer 512-Byte-Tabelle, der ein `pshufb` fuettert. Auf Zen 4 ist
das eine Abhaengigkeitskette von rund sechs Zyklen dort, wo records fast jeder
Block ist. Axion hat an derselben Stelle nichts Vergleichbares.

## A1: die Literalschleife der LZ4-Linie auf 64 Byte, mit vorgespanntem Zaehler

Das Profil auf Axion hat an einem incompressiblen 4-KiB-Block 77% der Zeit in
fuenf Instruktionen gefunden, und zwei davon fragten dasselbe:

    L(lz4_lit_blk):
        ldp     q0, q1, [x1], #32       5.4%
        stp     q0, q1, [x0], #32      57.7%
        subs    x2, x2, #32            11.5%
        cmp     x2, #32                 8.5%
        b.hs    L(lz4_lit_blk)

`subs` zieht ab und setzt Flaggen, `cmp` stellt danach dieselbe Frage an das
Ergebnis. Zieht man die Rundenbreite einmal beim Eintritt ab, ist der `subs` am
Fuss die Pruefung: der Borrow bedeutet genau "weniger als eine Runde uebrig".
Das `adds` am Ausgang stellt den echten Rest wieder her und ist zugleich der
Nulltest, den vorher ein `cbz` machte. Mit 64 statt 32 Byte je Runde werden aus
fuenf Instruktionen je 32 Byte drei.

### M2 Max, vier Runden, abwechselnd gemessen

| Zelle | alt | neu | Delta | alle Laeufe (GiB/s) |
|---|---|---|---|---|
| keva/noise_512 | 46.54 | 49.78 | **+7.0%** | alt 46.7 46.4 25.6 48.3 / neu 49.7 49.9 50.4 49.7 |
| keva/noise_4k | 64.15 | 69.20 | **+7.9%** | alt 65.4 63.6 63.1 64.8 / neu 69.1 68.5 69.3 69.5 |
| keva/noise_64k | 55.54 | 55.69 | +0.3% | alt 55.2 57.7 53.6 55.9 / neu 57.6 56.6 54.3 54.8 |
| liblz4/noise_512 | 46.70 | 45.77 | -2.0% | alt 47.2 47.5 26.4 46.2 / neu 45.7 45.9 46.4 45.6 |
| liblz4/noise_4k | 64.10 | 63.79 | -0.5% | alt 64.7 64.6 63.6 63.6 / neu 62.0 64.7 64.1 63.5 |
| liblz4/noise_64k | 56.03 | 56.11 | +0.2% | alt 55.1 56.5 55.5 58.7 / neu 55.5 57.4 56.7 54.7 |

Runde 3 hat beide Seiten gleichzeitig getroffen (25.6 und 26.4) -- eine
Stoerung der Maschine, kein Effekt der Revision. Ohne sie steht noise_512 bei
+6.4% gegen einen Kontrollabfall von 2.0%, also real aber schwaecher als der
Median sagt. noise_4k ist eindeutig: die Verteilungen ueberlappen nicht.

**noise_64k bei +0.3% ist die Gegenprobe.** Dort greift `LIT_MEMCPY 8192`, die
Schleife wird gar nicht betreten, und die Zelle bewegt sich nicht. Der Gewinn
liegt genau dort, wo die Theorie ihn hinlegt.

### Dass das eigene Format sich nicht bewegen kann

Die Schleife steht unter `#ifdef KEVA_LZ4`. Beide Revisionen mit `clang -c`
assembliert und die Objekte verglichen:

| Body | |
|---|---|
| `unpack.o` (even) | **identisch**, 3320 B |
| `unpack_wide.o` | **identisch**, 3400 B |
| `unpack_lz4.o` | geaendert, 4248 B |

### Was der Mutationstest gefunden hat

Vier Verfaelschungen gegen den Soak, jede einzeln:

| Mutation | Soak |
|---|---|
| zweites `ldp`/`stp` weg -- halbe Kopie je Runde | **rot** |
| `adds x2, x2, #48` statt `#64` -- Rest um 16 zu klein | **rot** |
| Eintrittsbias `#32` statt `#64` | gruen |
| `b.hs` -> `b.hi` | gruen |

`b.hi` ist kein Fehler: bei genau 64 Rest faellt die Schleife heraus und
`L(exact)` kopiert die 64 exakt. Richtige Ausgabe, nur langsamer.

Der Eintrittsbias ist einer. Mit `#32` betritt die Schleife ihren Rumpf schon
bei vierzig verbleibenden Bytes und kopiert dann vierundsechzig -- **bis zu 63
Bytes ueber den Eingabepuffer hinaus gelesen**. Die Ausgabe bleibt trotzdem
korrekt, weil die Extrabytes in den Ueberlaufschlupf fallen, den der Aufrufer
ohnehin abschneidet. Der Soak prueft, was herauskommt, und kann das deshalb
nicht sehen.

Die abgelieferte Fassung ist auf dieser Grenze sauber -- eine Runde laeuft nur
mit vierundsechzig Bytes in der Hand und liest vierundsechzig. Aber die
Abdeckung dafuer fehlt, und sie fehlt fuer jeden Pfad in dieser Datei, nicht nur
fuer diesen. Ein Test, der den gepackten Block ans Ende einer Seite legt und die
naechste nicht abbildet, wuerde jeden Uebergriff sofort mit SIGSEGV melden. Das
ist die naechste Luecke, die zugeht.
