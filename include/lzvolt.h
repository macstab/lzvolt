/*
 * lzvolt — LZ, variable.
 *
 * A byte-oriented LZ77 codec that picks its token layout per value, with
 * hand-written decoders per CPU part line. It reads and writes its own
 * format, and it also reads raw LZ4 blocks.
 *
 * Copyright The lzvolt authors.
 * SPDX-License-Identifier: Apache-2.0
 *
 * The stream format is specified in docs/FORMAT.md and is stable: a stream
 * written by any version of this library is readable by any other. Test
 * vectors for a second implementation are in docs/vectors.txt.
 *
 *
 * HOW TO USE IT
 *
 *   Compressing. Ask for a buffer, compress into it, and handle the refusal:
 *
 *       size_t  cap = lzvolt_compress_bound(len);
 *       uint8_t *buf = malloc(cap);
 *       int64_t  n  = lzvolt_compress(data, len, buf, cap);
 *
 *       if (n > 0)  { store(buf, (size_t)n); }     // a stream
 *       else if (n == LZVOLT_NOT_COMPRESSIBLE)
 *                   { store(data, len); }          // store it as it is
 *       else        { fail(lzvolt_strerror(n)); }
 *
 *   A refusal is not an error. This encoder declines anything it cannot
 *   shrink by an eighth, because reading a packed value costs four to five
 *   times what reading a stored one does — so saving a few bytes is a bad
 *   trade. You store the original bytes and skip the decode entirely.
 *
 *   Decompressing. The stream carries its own length:
 *
 *       int64_t size = lzvolt_declared_size(buf, n);
 *       if (size < 0) { fail(lzvolt_strerror(size)); }
 *
 *       uint8_t *out = malloc(lzvolt_decompress_bound((size_t)size));
 *       int64_t  got = lzvolt_decompress(buf, n, out, lzvolt_decompress_bound(size));
 *
 *
 * WHAT THESE FUNCTIONS PROMISE
 *
 *   Buffers are yours. Nothing here allocates on your behalf and nothing
 *   needs freeing, so there is no lzvolt_free() and no ownership to track.
 *
 *   Decompression writes straight into your buffer. There is no copy on the
 *   way out. That is deliberate: this decoder reads at over 13 GiB/s, and a
 *   memcpy of the result would cost a fifth of the decode.
 *
 *   Compression copies once, internally. The encoder builds its stream in a
 *   per-thread scratch buffer and copies the compressed bytes — not the
 *   input — into yours. It is a few percent of a compress, and it is the one
 *   place this ABI is not zero-copy.
 *
 *   Lengths are size_t; results are int64_t. Zero or more is the number of
 *   bytes written. Negative is an LZVOLT_E_* code, and nothing was written.
 *
 *   Bad arguments give codes, not crashes. A NULL pointer with a non-zero
 *   length is LZVOLT_E_NULL. A NULL pointer with a zero length is fine — it
 *   is what an empty buffer looks like.
 *
 *   Malformed input gives codes, not crashes. Every stream is treated as
 *   hostile; see the list of what a decoder must reject in docs/FORMAT.md.
 *
 *   Nothing throws. No exception, no unwinding, no longjmp crosses this
 *   boundary.
 *
 *   Every function is thread-safe and none of them keeps shared state. The
 *   compressor's scratch buffer is per-thread.
 */

#ifndef LZVOLT_H
#define LZVOLT_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* The input does not compress. Not an error: store the bytes as they are.
 * No valid stream is zero bytes long, so this cannot be a real result. */
#define LZVOLT_NOT_COMPRESSIBLE    ((int64_t)0)

/* A pointer was NULL where a length said there would be bytes. */
#define LZVOLT_E_NULL              ((int64_t)-1)
/* The stream ends in the middle of a field. */
#define LZVOLT_E_TRUNCATED         ((int64_t)-2)
/* A backward reference points before the start of the output. */
#define LZVOLT_E_BAD_OFFSET        ((int64_t)-3)
/* The stream does not produce the length it declares. */
#define LZVOLT_E_LENGTH_MISMATCH   ((int64_t)-4)
/* The declared length is larger than any value may be. */
#define LZVOLT_E_TOO_LARGE         ((int64_t)-5)
/* The header spends more bytes than its length needs. One length has one
 * encoding, so that packed bytes can be compared for equality. */
#define LZVOLT_E_NON_CANONICAL     ((int64_t)-6)
/* The output buffer cannot hold the declared length. */
#define LZVOLT_E_OUTPUT_TOO_SMALL  ((int64_t)-7)

/* The largest value this format can carry: 2^29 - 1 bytes. It is the header's
 * own ceiling — four header bytes hold 29 bits of length and there is no
 * fifth class — so no stream can declare more. */
#define LZVOLT_MAX_SIZE            ((size_t)536870911)

/* A match never reaches further back than this. */
#define LZVOLT_WINDOW              ((size_t)65535)


/* ---- sizing ---------------------------------------------------------- */

/* The largest stream lzvolt_compress() can produce from src_len bytes.
 * A buffer of this size never runs out. */
size_t lzvolt_compress_bound(size_t src_len);

/* How much room lzvolt_decompress() wants for a value of `declared` bytes.
 *
 * Exactly `declared` decodes the same bytes and is safe. This much lets the
 * decoders copy in wide blocks instead of trimming the last one, which on
 * incompressible 4 KiB was measured at 34%. */
size_t lzvolt_decompress_bound(size_t declared);


/* ---- the codec ------------------------------------------------------- */

/* Compress src into dst.
 *
 * Returns bytes written, LZVOLT_NOT_COMPRESSIBLE (0) if the encoder declined,
 * or a negative LZVOLT_E_* code. dst_cap should be lzvolt_compress_bound().
 *
 * src and dst must not overlap. */
int64_t lzvolt_compress(const uint8_t *src, size_t src_len,
                        uint8_t *dst, size_t dst_cap);

/* The uncompressed length the stream declares, without decoding it.
 *
 * Validates the header on the way, so a stream this rejects is one
 * lzvolt_decompress() would reject too. Returns the length, or a negative
 * LZVOLT_E_* code. */
int64_t lzvolt_declared_size(const uint8_t *src, size_t src_len);

/* Decompress src into dst, writing directly into it.
 *
 * Returns bytes written or a negative LZVOLT_E_* code. dst_cap must be at
 * least lzvolt_declared_size(), and should be lzvolt_decompress_bound() of it.
 *
 * src and dst must not overlap. */
int64_t lzvolt_decompress(const uint8_t *src, size_t src_len,
                          uint8_t *dst, size_t dst_cap);

/* Decompress a raw LZ4 block — no frame, no magic, no checksum.
 *
 * This is what LZ4_compress_default() writes and LZ4_decompress_safe() reads.
 * An LZ4 block does not carry its uncompressed size, which is why `declared`
 * is an argument here and not above.
 *
 * The reverse does not work: this format's streams are not LZ4 blocks and a
 * conforming LZ4 decoder will reject some of them. See docs/FORMAT.md.
 *
 * Returns bytes written or a negative LZVOLT_E_* code. */
int64_t lzvolt_decompress_lz4(const uint8_t *src, size_t src_len,
                              uint8_t *dst, size_t dst_cap, size_t declared);


/* ---- identifying the build ------------------------------------------- */

/* The code path this build decodes with: "aarch64 assembly", "x86-64
 * assembly", or the portable fallback. Static; do not free.
 *
 * Worth recording next to any measurement — a number that cannot be traced
 * to the path that produced it says nothing. */
const char *lzvolt_backend(void);

/* This library's version, e.g. "1.0.0". Static; do not free. */
const char *lzvolt_version(void);

/* A short description of an LZVOLT_E_* code. Never NULL, static; do not
 * free. Unknown codes get a description too. */
const char *lzvolt_strerror(int64_t code);

#ifdef __cplusplus
} /* extern "C" */
#endif

#endif /* LZVOLT_H */
