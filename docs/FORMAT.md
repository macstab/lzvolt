# The lzvolt stream format

**Version 1.** This document is normative. Where it and any implementation
disagree, including the reference implementation in this repository, the
implementation is wrong.

A stream is self-contained: it carries its own uncompressed length, and nothing
outside it is needed to decode it. There is no frame, no magic number, no
checksum, and no dictionary. What follows describes the whole format.

## Conventions

**MUST**, **MUST NOT**, **SHOULD** and **MAY** are used as in RFC 2119.

All multi-byte integers are **little-endian**, except the length in the header,
which is big-endian and is described as such where it appears. Bit 7 is the most
significant bit of a byte. All lengths and positions are counts of bytes.

Two terms are used throughout:

- *declared* — the uncompressed length the header carries.
- *produced* — how many bytes the decoder has written so far.

Decoding is finished when *produced* equals *declared*. Almost every rule below
is ultimately about keeping those two in step.

## Overview

```
stream  := header body [trailer]
body    := section [section]
section := block...
block   := token [literal-chain] literals [offset] [match-chain]
```

A stream has one or two *sections*. Each section interprets its blocks' tokens
under one of two bit layouts, called *splits*. A stream with two sections is
*hybrid*; its header says so and its trailer says where the second section
begins.

The trailer is present if and only if the header's hybrid flag is set.

## Header

One to four bytes. The first two bits give the length, so a decoder knows how
much header there is before it interprets any of it.

```
byte 0:      c c h p p p p p     c = class; the header is c+1 bytes long
                                 h = hybrid flag (class 0: part of p)
                                 p = payload, the most significant bits
                                     of declared
bytes 1..c:  the remaining bits of declared, most significant first
```

| class | header bytes | hybrid flag | payload bits | *declared* range |
|---|---|---|---|---|
| 0 | 1 | none | 6 | 0 … 63 |
| 1 | 2 | bit 5 | 5 + 8 = 13 | 64 … 8 191 |
| 2 | 3 | bit 5 | 5 + 16 = 21 | 8 192 … 2 097 151 |
| 3 | 4 | bit 5 | 5 + 24 = 29 | 2 097 152 … 536 870 911 |

### The class is the shortest that fits

An encoder **MUST** use the shortest class that can hold *declared*, and a
decoder **MUST** reject a stream that uses a longer one.

Such a stream would decode correctly — the length reads back the same either
way — and that is precisely why the rule exists rather than why it could be
relaxed. Without it one length has up to four encodings, so two byte strings
can mean the same value while comparing as different. Anything that hashes or
compares packed bytes to recognise a value it has already seen, a cache
deduplicating entries for instance, silently stops working. Nothing downstream
of the decoder can detect the difference, which leaves the decoder as the only
place it can be caught.

A decoder needs one comparison for this: *declared* must be at least the
smallest value its class is for — 64, 8 192, or 2 097 152. Class 0 has nothing
below it and needs no check.

### A class-0 stream is never hybrid

Class 0 has no hybrid flag, so a stream of 63 bytes or fewer **MUST NOT** have
two sections, and its bit 5 is simply payload. An encoder that wants a switch in
a value that small cannot have one.

This costs nothing in practice, but the reason is worth separating from the
rule: it is not that a small value *may not* switch, it is that no encoder has
ever wanted to. A section pays for itself only across enough blocks to amortise
the trailer, and the trailer alone is four bytes of a value that has at most 63.

## Sections and the token split

Every block's token is eight bits, divided one of two ways. Which one applies is
a property of the *section*, not of the block — within a section, every token is
read the same way.

**Even split.** This is also the LZ4 block token layout.

```
  7 6 5 4 3 2 1 0
 [ L L L L|M M M M ]     L = literal length, 0..15
                         M = match length,   0..15
```

**Wide split.** Two bits of literal length buy a fifth match-length bit and a
repeat flag.

```
  7 6 5 4 3 2 1 0
 [ L L|M M M M M|R ]     L = literal length, 0..3
                         M = match length,   0..31
                         R = repeat flag
```

A stream that is not hybrid uses the **even split throughout**. This is
normative, and it is narrower than the layouts alone would allow: the wide split
is reachable only as the second section of a hybrid stream.

### The trailer

A hybrid stream begins in the even split and changes to the wide split exactly
once, at a position the trailer gives:

```
trailer := in_at out_at        each w bytes, little-endian

w = 2   when declared <= 65 536
w = 4   otherwise
```

- `in_at` is a byte offset into the body: the first byte of the second section.
- `out_at` is *produced* at the moment of the switch.

The width follows from *declared* alone, so a decoder knows it before it reads
the trailer — it is not a flag, because a flag is another bit to get wrong.

A decoder **MUST** reject a trailer in which `in_at` is greater than the body
length, or `out_at` is zero, or `out_at` is greater than or equal to *declared*.

For `w = 2` the bound on `out_at` is exactly tight, and an implementer who
writes `<` where this document writes `<=` will get exactly one length wrong:
`out_at` is strictly below *declared*, so at *declared* = 65 536 its largest
value is 65 535, which is the last value two bytes hold.

`in_at` needs a rule of its own, because nothing else bounds it. An encoder
**MUST NOT** write a hybrid stream whose body is longer than its trailer width
can address — 65 535 bytes when `w = 2` — because the switch point would not be
expressible. Compressed bodies are far shorter than this in every case that
arises in practice, but the format does not oblige a stream to be smaller than
its input, so the limit has to be stated rather than assumed.

This one is an obligation on encoders and not a rejection for decoders, which
is unusual enough here to be worth the sentence. A decoder cannot detect the
violation: the truncated `in_at` it reads back is itself a legal position, so
the stream is internally consistent and decodes to the wrong bytes rather than
to an error.

### Across the boundary

The two sections are decoded one after the other, each reading its tokens under
its own split. They are not independent: a match in the second section **MAY**
reach back into output the first section produced. Only the token layout changes
at the boundary, not the output.

One thing does reset. The remembered offset that `R` refers to (below) belongs
to the section, so the second section starts without one.

## Blocks

A section is a sequence of blocks with no count in front of them. Blocks are
read until *produced* reaches *declared*.

```
block := token [literal-chain] literals [offset] [match-chain]
```

### Extension chains

Both length fields in the token are small, and both escape the same way. When a
field is at its maximum, the value continues in a chain of bytes immediately
after the point described below; when it is below its maximum, that is the whole
value and no chain is present.

A chain is read by adding bytes to a running total: a byte of 255 means another
byte follows, and any other value — including zero — ends the chain. The total
is then added to the field.

Two consequences are worth making explicit, because both are easy to get wrong:

- A field at its maximum is **always** followed by a chain, even when the chain
  adds nothing. A literal run of exactly 15 under the even split is `L = 15`
  followed by a single zero byte; there is no shorter encoding of it.
- Reaching the maximum tells a decoder nothing about how long the chain is. A
  decoder **MUST** reject a chain that runs past the end of the input, and one
  whose total would overflow the arithmetic it is accumulated in.

### Literal length and literals

`L` is at its maximum at 15 under the even split and at 3 under the wide split.
When it is, a chain follows the token; the literal length is `L` plus that
chain's total.

Exactly that many bytes follow, and they are copied to the output unchanged.

A decoder **MUST** reject a literal run that reaches past the end of the input,
and one that would carry *produced* past *declared*.

### Offset

Two bytes, little-endian. It is present unless either

- the block has no match, which is the case for a block that reaches *declared*
  on its literals alone, or
- the section uses the wide split and the token's `R` flag is set.

When `R` is set, the block is decoded with the offset of the previous block of
the same section. A stream **MUST NOT** set `R` on a section's first block,
because there is no previous offset for it to name.

An offset of zero is invalid. So is an offset greater than *produced*, which
would reach back past the start of the output. A decoder **MUST** reject both.

Those two rules are enough to enforce the one above them, which is why a decoder
needs no separate check for it: if the remembered offset starts each section at
zero, then a first block with `R` set is a block with an offset of zero, and it
is already refused. The reference decoders rely on exactly this. It is the only
place in the format where two rules are load-bearing together, so it is worth
naming rather than leaving to be rediscovered.

The offset is at most 65 535. That is the format's window: a match never reaches
further back than 64 KiB, whatever the size of the value.

### Match length

`M` is at its maximum at 15 under the even split and at 31 under the wide split.
When it is, a chain follows the offset — read exactly as the literal chain is.

```
match length = 4 + M + chain total      (chain present)
             = 4 + M                    (otherwise)
```

The four is the minimum match. Below four bytes a match costs more to encode
than it saves, so no shorter one exists to express.

A match **MAY** overlap the bytes it produces: an offset of 1 with a length of
200 is one byte repeated 200 times. A decoder therefore **MUST NOT** copy a
match with a load wider than the offset unless it has first established that the
offset is at least that wide — a blockwise copy that reads ahead of what it has
written will read bytes this match is supposed to be producing.

### Where a stream ends

Decoding ends when *produced* equals *declared*. There is no terminator, and no
block is distinguished as the last one.

A stream **MAY** end immediately after a match, and a decoder **MUST** accept
one that does. The reference encoder instead runs its matches to the final byte
and closes with a token carrying no literals and no match, but that is its
choice and not a rule; a conforming encoder may stop on the match itself.

A decoder **MUST** reject a stream whose body ends before *declared* is reached,
and one in which any block would carry *produced* past it.

## Limits

| | |
|---|---|
| Maximum *declared* | 536 870 911 bytes (2²⁹ − 1) |
| Maximum offset | 65 535 bytes |
| Minimum match | 4 bytes |
| Maximum literal run | bounded only by *declared* |
| Maximum match length | bounded only by *declared* |

The maximum length is the header's own ceiling rather than a separately chosen
one: four header bytes hold 29 bits of payload, and there is no fifth class. No
stream can declare more, so a decoder cannot be handed one that does.

A decoder is free to impose a smaller limit of its own — the memory it is
willing to commit to one value — and **SHOULD** report that as its own refusal
rather than as a malformed stream. The reference decoder allows 512 MiB, which
sits just above 2²⁹ − 1 and therefore never refuses a stream on that ground;
the limit is live only on the LZ4 entry point, where the length is supplied by
the caller instead of read from the bytes.

## What a decoder must reject

Collected, because a decoder reading bytes off a socket has to treat every one
of them as hostile. Nothing in this list is a heuristic; each is a definite
property of a malformed stream.

1. A header class longer than *declared* needs.
2. An extension chain that runs past the end of the input, or whose total
   overflows.
3. A literal run that reaches past the end of the input.
4. Any block that would carry *produced* past *declared*.
5. An offset of zero, or one greater than *produced*.
6. `R` set on the first block of a section — which follows from 5.
7. A trailer whose `in_at` exceeds the body, or whose `out_at` is zero or not
   below *declared*.
8. A body that ends before *produced* reaches *declared*.

The reference decoder is held to all eight by fuzzing every truncation and every
single-bit flip of a packed stream, by thousands of multi-byte corruptions, by
streams that were never packed at all, and by running the whole set under Miri.
Its unchecked copies make this the only thing standing between a malformed
stream and memory unsafety, so the tests assert the absence of out-of-bounds
access and not merely the presence of an error. See `src/format.rs`.

## What an encoder may choose

Everything not stated above. The following in particular are **implementation
choices that cannot change what a conforming decoder does**, and an encoder that
decides them differently still produces valid streams:

- when to emit a match instead of extending a literal run;
- how it searches for matches at all — table size, chain depth, lazy matching,
  skip heuristics;
- whether it uses a second section, and where it puts the switch;
- whether it sets `R` on a block whose offset does in fact repeat;
- whether it compresses a given input at all.

That last one deserves stating plainly: **an encoder may refuse.** `lzvolt`'s
does, for input that does not compress, and hands the original bytes back to the
caller rather than wrapping them in a stream.

Its two refusal rules are **not** part of this format, and a decoder **MUST
NOT** infer either of them from the streams it happens to have seen:

- it refuses input shorter than 8 bytes;
- it refuses a result that does not save at least an eighth of the input, and at
  least 8 bytes.

The second is a space rule with a speed reason behind it: reading a packed value
costs four to five times what reading a stored one does, at every size, so
saving a single byte is a bad trade even though it is a legal stream. Another
conforming encoder may make that trade, and a decoder has to read what it
produces. Between them the two rules mean no stream this encoder writes declares
a length below 8 — which is a fact about the encoder and not about the format,
and `a_length_the_encoder_would_never_emit_still_decodes` in `src/format.rs`
hand-builds every one of those lengths to keep the decoder honest about the
difference.

Thresholds inside the reference decoder — when it calls `memcpy`, how wide its
copy loop is, where it aligns, which of its per-processor bodies runs — are not
part of this format either. They do not change a byte of any stream, and they
are chosen by measurement; see `docs/MEASUREMENTS.md`.

## Relationship to LZ4

The even split *is* the LZ4 block token layout. Extended lengths are the same
chains of 255, the offset is the same two little-endian bytes, and match lengths
are stored the same four less. An LZ4 block is therefore a valid lzvolt body,
and `lzvolt::decompress_lz4` reads one directly.

The converse does not hold, and the reason is worth being exact about. LZ4
constrains where a block may end: its last match must not begin within twelve
bytes of the end, and its final literal run must be at least five bytes. This
format requires neither. A stream may run matches to the final byte, and may end
on a match with nothing after it — both of which a conforming LZ4 decoder has no
way to interpret. A hybrid stream's second section is not LZ4's token layout at
all.

So: this format reads LZ4. LZ4 does not read this format, and it is right not
to.

## Test vectors

`docs/vectors.txt` carries sixteen streams and what a decoder must do with each
— eleven to decode, five to refuse. Between them they cover all four header
classes, both splits, both kinds of extension chain, an overlapping match, a
stream ending on a match, the hybrid switch, and four of the eight rejections
above.

They are **decoder** vectors, deliberately. Nothing pins what an encoder emits
for a given input, because that is the encoder's choice; a file that pinned it
would make a conforming implementation look wrong for making a different one.

The file describes its own format in its header: one field per line, hex bytes,
records separated by blank lines. A parser for it is about twenty lines in any
language. Regenerate it with `cargo run --example make_vectors`; the committed
file is re-read and re-checked on every test run by
`the_published_vectors_hold`, so it cannot quietly fall behind the code.
