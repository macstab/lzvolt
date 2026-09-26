# The lzvolt stream format

Version 1. This document is normative: where it and any implementation
disagree, the implementation is wrong.

It was written by reading the encoder and the decoders, and writing it was worth
it for more than the document. It found three source comments that no longer
described the code — noted where they occur — three tests still building headers
in a format two revisions old, and two decoder guards that cannot fire. None of
that was visible from a green test run, because every one of them was green.

A stream is self-contained. It carries its own uncompressed length, and nothing
outside it is needed to decode it.

## Conventions

**MUST**, **MUST NOT** and **MAY** carry their usual weight. All multi-byte
integers are **little-endian** except the length in the header, which is
big-endian and says so where it appears. Bit 7 is the most significant bit of a
byte.

Throughout, *declared* is the uncompressed length the header carries, and
*produced* is the number of bytes the decoder has written so far.

## The stream

```
stream := header body [trailer]
```

The trailer is present if and only if the header's hybrid flag is set.

## Header

One to four bytes. The first two bits say how many, so a decoder knows the
header's length before it examines any of it.

```
byte 0:  c c h p p p p p        c = class, header is c+1 bytes
                                h = hybrid  (class 0: part of p)
                                p = payload, the top bits of declared
bytes 1..c:  the remaining bits of declared, most significant first
```

| class | header bytes | hybrid bit | payload bits | declared range |
|---|---|---|---|---|
| 0 | 1 | — | 6 | 0 … 63 |
| 1 | 2 | bit 5 | 5 + 8 = 13 | 64 … 8 191 |
| 2 | 3 | bit 5 | 5 + 16 = 21 | 8 192 … 2 097 151 |
| 3 | 4 | bit 5 | 5 + 24 = 29 | 2 097 152 … 536 870 911 |

An encoder **MUST** use the shortest class that holds *declared*, and a decoder
**SHOULD** reject a stream whose class is longer than necessary. Without that
rule one length has four encodings, and anything that compares or hashes packed
bytes for equality — a cache deduplicating values, for instance — would see two
identical values as different.

> **Known deviation.** The reference decoder does not check this today: it reads
> the class, masks, shifts, and accepts. Streams its own encoder produces are
> always canonical, so the gap is only reachable from a third-party or hostile
> encoder, and it costs correctness of *equality*, never of decoding. It is
> recorded here rather than quietly weakened to match.

Class 0 has no hybrid bit and needs none: a switch is only permitted after
sixteen blocks, every block produces at least four bytes, so no value of 63
bytes or fewer can carry one.

> The module comment in `src/format.rs` described this as
> `varint(declared << 2 | hybrid << 1 | split)`. That was the format once and is
> not now. The split bit in particular is gone — see below.

## Body

A sequence of blocks. There is no block count; the body ends when *produced*
reaches *declared*.

```
block := token [literal-extension] literals [offset] [match-extension]
```

### Token

The token's eight bits are divided one of two ways. Which one applies is the
*split*, and it is a property of the section a block belongs to, not of the
block.

**Even split** — also the LZ4 token layout:

```
  7 6 5 4 3 2 1 0
 [ L L L L|M M M M ]     L = literal length, 0..15
                         M = match length,   0..15
```

**Wide split**:

```
  7 6 5 4 3 2 1 0
 [ L L|M M M M M|R ]     L = literal length, 0..3
                         M = match length,   0..31
                         R = repeat flag
```

> A comment in `src/format.rs` said the wide split "spends all eight token bits
> on lengths". It is the even split that does; the wide split is the one with
> the repeat bit.

### Literal length

If `L` is at its maximum for the split — 15 for even, 3 for wide — the true
length continues in an extension chain immediately after the token. Otherwise
the literal length is `L`.

An extension chain is a sequence of bytes. Each byte is added to a running
total; a byte of 255 means another byte follows, and any other value ends the
chain. The chain's total is added to the token's field.

A decoder **MUST** reject a chain whose total would overflow, and **MUST**
reject a literal run that would carry *produced* past *declared*.

Exactly that many literal bytes follow the chain. They are copied to the output
unchanged.

### Offset

Two bytes, little-endian, present unless either

- the block is the final one, which has no match at all, or
- the split is wide and the token's `R` bit is set.

When `R` is set the block **MUST** be decoded with the offset of the previous
block in the same section. A stream **MUST NOT** set `R` on the first block of a
section, since there is no previous offset for it to name.

A decoder satisfies this without a separate check by starting each section with
its remembered offset set to zero: a zero offset is invalid, so the rule below
rejects such a block. The reference decoders do exactly that, and it is worth
naming because it is the only place where two rules are load-bearing together.

An offset of 0 is invalid. An offset greater than *produced* is invalid: it
would reach before the start of the output. A decoder **MUST** reject both.

The offset is at most 65 535, which is the format's window.

### Match length

If `M` is at its maximum for the split — 15 for even, 31 for wide — an extension
chain follows the offset, read exactly as the literal chain is. The match length
is then

```
4 + M + chain-total          (chain present)
4 + M                        (otherwise)
```

The four is the minimum match: below four bytes a match costs more than it
saves, so no shorter one is encodable.

A match **MAY** overlap the bytes it produces — an offset of 1 with a length of
200 is a run of one byte repeated — so a decoder **MUST NOT** copy a match with
a wide load that reads ahead of what it has written, unless it has established
that the offset is at least as large as that load.

### Termination

The final block carries literals and no match: no offset, no match extension.
Decoding ends when *produced* equals *declared*.

A decoder **MUST** reject a stream where the body ends before *declared* is
reached, and one where a block would carry *produced* past it.

## The split, and the trailer

A stream that is not hybrid uses the **even split throughout**. This is
normative and it is narrower than the format could be: the wide split is
reachable only through a hybrid switch.

A hybrid stream begins in the even split and changes to the wide split exactly
once. The trailer says where:

```
trailer := in_at out_at        each w bytes, little-endian
w = 2  when declared <= 65 536
w = 4  otherwise
```

`in_at` is a byte offset into the body: the first byte of the wide section.
`out_at` is the number of output bytes produced before the switch.

The width follows from *declared* alone, so a decoder knows it before reading
the trailer. That both fields fit in the narrow case is worth spelling out,
because the bound is exactly tight and an implementer who writes `<` where this
says `<=` will get it wrong at one length:

- `out_at` is strictly below *declared*, so at the boundary *declared* = 65 536
  its largest value is 65 535 — the last value two bytes hold.
- `in_at` is below the body length, and the body is shorter than *declared*
  (see below), so it is smaller still.

A decoder **MUST** reject a trailer with `in_at > body length`, with
`out_at == 0`, or with `out_at >= declared`.

The two sections are decoded independently, each starting at its own split, and
the second **MUST** be able to reference output the first produced: a match in
the wide section may reach back across the boundary.

## Limits

| | |
|---|---|
| Maximum declared length | 536 870 911 bytes (2²⁹ − 1) |
| Maximum offset | 65 535 |
| Minimum match | 4 |
| Maximum literal run | bounded only by *declared* |
| Maximum match length | bounded only by *declared* |

The maximum length is the header's, not a separately chosen ceiling: four bytes
of header hold 29 bits and there is no fifth class. A decoder therefore cannot
be handed a stream declaring more than this, whatever it does about it — and the
reference decoder's own 512 MiB buffer guard sits just above 2²⁹ − 1, so on the
`decompress` path that guard cannot fire. It is live only on `decompress_lz4`,
where the length is an argument rather than something read from the bytes.

## What an encoder is free to choose

Everything not named above. In particular the following are **implementation
choices that do not change what a conforming decoder does**, and an encoder that
picks differently still produces valid streams:

- when to emit a match rather than extend a literal run,
- how it searches for matches at all — hash table size, chain depth, lazy
  matching, skip heuristics,
- whether it uses the hybrid switch, and where it places it,
- whether it sets `R` on a block whose offset does repeat,
- whether it compresses a given input at all.

That last one is worth stating plainly: **an encoder may refuse.** `lzvolt`'s
does, for data that does not compress, and returns the input to the caller
untouched rather than wrapping it. A stream is therefore never larger than its
input, because a stream that would be is never produced.

`lzvolt`'s encoder refuses on two rules that are **not** part of this format, and
a decoder **MUST NOT** infer either of them from the streams it happens to see:

- it refuses inputs shorter than 8 bytes, and
- it refuses a result that does not save at least an eighth of the input and at
  least 8 bytes — the bar is space, because a packed value costs four to five
  times a stored one to read, so saving a single byte is a bad trade.

The second rule is what bounds the body: every stream this encoder writes is at
most ⅞ of its input, which is what the trailer's two-byte case rests on. Another
conforming encoder may keep a stream that saves one byte, and a decoder has to
read it. Both rules are also why no stream from this encoder declares a length
below 8 — and `a_length_the_encoder_would_never_emit_still_decodes` in
`src/format.rs` hand-builds every one of those lengths to hold the decoder to
the format rather than to its own encoder's habits.

Thresholds in the reference implementation — when the decoder calls `memcpy`,
how wide its copy loop is, where it aligns — are not part of this format. They
do not change a byte of any stream.

## Relationship to LZ4

The even split *is* the LZ4 block token layout, and an LZ4 block decodes
correctly as an lzvolt body. `lzvolt::decompress_lz4` exists for exactly that.

The reverse does not hold, and it is worth being precise about why. LZ4
constrains where a block may end: the last match must not begin within twelve
bytes of the end, and the final literal run must be at least five bytes. This
format enforces neither. Its encoder runs matches to the final byte and then
writes a token with no literals and no match, which LZ4 has no way to
interpret. A conforming LZ4 decoder **will** reject some valid lzvolt streams,
and it is expected to.

So: read LZ4 with this, do not hand this to LZ4.

## What a decoder must validate

Collected, because a decoder reading bytes off a socket has to treat all of
them as hostile:

1. The header's class is the shortest that holds its length.
2. *declared* does not exceed the maximum.
3. Every extension chain terminates within the input and does not overflow.
4. No literal run reaches past the end of the input.
5. No block carries *produced* past *declared*.
6. Every offset is at least 1 and at most *produced*.
7. `R` is not set on the first block of a section.
8. The trailer's two positions are within the body and the output.
9. The body ends exactly when *produced* reaches *declared*.

The reference decoder is checked against all nine by fuzzing every truncation
and every single-bit flip of a packed stream, by thousands of multi-byte
corruptions, by streams that were never packed at all, and by running the lot
under Miri. See `src/format.rs` and `docs/MEASUREMENTS.md`.
