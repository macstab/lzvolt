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
