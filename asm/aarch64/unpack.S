/*
 * keva_unpack -- the decoder, one call per value.
 *
 *   uint32_t keva_unpack(const uint8_t *src, uintptr_t src_len,
 *                        uint8_t *dst, uintptr_t dst_cap,
 *                        uintptr_t declared);
 *
 * `src` points past the length header, which the caller has already decoded
 * into `declared`. Returns `declared` on success, or 0 to mean "this needs the
 * portable decoder" -- either the stream is malformed, or the buffer is too
 * small for the checks below to be sufficient. The caller then runs the Rust
 * decoder, which produces the precise error. A stream only fails when it is
 * corrupt, so paying for that twice costs nothing that matters, and it keeps
 * every error message in one place.
 *
 * AAPCS64: x0 src, x1 src_len, x2 dst, x3 dst_cap, x4 declared. Result in w0.
 *
 * # Why the decoder is worth hand-writing
 *
 * The packer runs once per write, the decoder on every read. It is also the
 * shape assembly is good at: two copies whose lengths the compiler cannot
 * bound, so it emits calls to memcpy and re-asks about capacity at every block,
 * and almost nothing else.
 *
 * # The capacity check that is not here
 *
 * The Rust decoder asks `produced + 64 <= capacity` once per block. Requiring
 * `dst_cap >= declared + 64` on entry makes that question answer itself, since
 * `produced < declared` holds at every block: `produced + 64 < declared + 64 <=
 * dst_cap`. Every store below is covered by that single check, and the loop
 * carries none.
 *
 * The same argument covers the wide copies. A literal run is checked against
 * `declared` before it is copied, and a copy rounded up to a whole block
 * overruns by at most 31 -- inside the 64 bytes of slack. Bytes written past
 * `produced` are either overwritten by the next block or fall in the slack the
 * caller trims, so nothing ever reads them.
 *
 * # The input bound that is not here either
 *
 * The fast path reads at most one token, fourteen literals, two offset bytes
 * and one 32-byte block: 49 bytes from the cursor. Rather than recompute that
 * per block, `.Lentry` derives the cursor at which it stops holding and the
 * loop compares against it. Past that point the tail runs on the exact path.
 *
 *   OVERRUN 32   FAST_LITERAL 16   MIN_MATCH 4   MAX_UNPACKED 512 MiB
 */

#include "../common.h"

    .text
    .p2align 4
    .globl KEVA_SYM(keva_unpack)

/*
 * Register map, held across the whole call:
 *
 *   x19 src            x23 at, the input cursor
 *   x20 src_len        x24 produced, the output cursor
 *   x21 dst            x25 cursor past which the fast path is unsafe
 *   x22 declared       x26 the token's match nibble, across helper calls
 *
 * The helpers below are called from the loop, so what each one is allowed to
 * destroy is part of its contract:
 *
 *   .Lexact  x0, x1, x2, x4, x6      .Lrepeat  x0-x7
 *
 * The extended-length reader and the block copier used to be two more of these
 * and are now written out at each of their four sites. On data that compresses
 * well they ran once or twice per block, and a call for them is a branch out, a
 * branch back and a return-stack slot -- which measured 18% of the decoder on
 * such data. liblz4 carries both as self-loops inside its decode loop.
 *
 * Everything the loop needs across a call therefore lives in x11-x17 or in a
 * callee-saved register.
 */
KEVA_SYM(keva_unpack):
    stp     x29, x30, [sp, #-96]!
    stp     x19, x20, [sp, #16]
    stp     x21, x22, [sp, #32]
    stp     x23, x24, [sp, #48]
    stp     x25, x26, [sp, #64]
    stp     x27, x28, [sp, #80]
    mov     x29, sp

    mov     x19, x0
    mov     x20, x1
    mov     x21, x2
    mov     x22, x4

.Lentry:
    // The one capacity check. Everything below leans on it.
    add     x5, x22, #64
    cmp     x3, x5
    b.lo    .Lfail

    mov     x23, #0                     // at
    mov     x24, #0                     // produced

    /*
     * The zone the fast loop runs in, as two limits computed once.
     *
     * Inside it a block needs no bound test of its own: the input cursor stays
     * far enough from the end that a token, fourteen literals, two offset bytes
     * and a 32-byte block all land inside, and the output cursor stays far
     * enough that the at-most 32 bytes a fast block produces cannot reach the
     * declared length. Both clamp to zero for a value too small to hold one,
     * which then runs entirely on the checked loop.
     */
    subs    x25, x20, #48               // input limit
    csel    x25, x25, xzr, hs
    subs    x28, x22, #64               // output limit
    csel    x28, x28, xzr, hs

/*
 * Two loops over the same format, and that is the whole shape of this file.
 *
 * The fast one runs while both cursors sit inside the zone above. It asks
 * nothing about the end of the input, nothing about the end of the output and
 * nothing about the declared length -- the zone answered all three before the
 * loop began. What is left per block is the zone itself and the two nibbles.
 *
 * A saturated nibble is not a reason to leave. It is data-dependent and common
 * -- matches run long on data that repeats, literal runs run long on data that
 * does not -- so it drops into the general block writer and comes straight
 * back. Only a *boundary* leaves, and it leaves for good.
 *
 * As one loop, the boundary question was asked by every block for the whole
 * length of the value. liblz4 carries two decoders for exactly this reason, and
 * it is why lifting single instructions out of it -- `ccmp`, post-indexed
 * cursors, precomputed limits -- moved nothing here: the cost was never in the
 * instructions, it was in where the decision lived.
 */
    .p2align 4
.Lfast:
    cmp     x23, x25
    b.hs    .Ltail
    cmp     x24, x28
    b.hs    .Ltail

    ldrb    w4, [x19, x23]              // token
    lsr     w5, w4, #4                  // literal nibble
    and     w26, w4, #15                // match nibble, kept across helpers

    cmp     w5, #15
    b.eq    .Lslow
    cmp     w26, #15
    b.eq    .Lslow

    /* ---- the common block -------------------------------------------- */
    add     x7, x23, #1                 // literals start here
    add     x8, x7, x5                  // and the offset follows them
    ldrh    w11, [x19, x8]              // offset
    add     x14, x26, #4                // match length
    add     x16, x24, x5                // output position after the literals
    add     x12, x16, x14               // and after the match

    // Only the offset is still worth asking about. A fast block lays down at
    // most fourteen literals and eighteen match bytes, and the zone holds the
    // cursor sixty-four short of the declared length, so it cannot overrun.
    cbz     w11, .Lfail
    cmp     x11, x16
    b.hi    .Lfail                      // reaches back before the output

    // A short run carries at most fourteen literals, so one 16-byte move covers
    // it whatever the length. Trimming would cost a branch to save nothing: the
    // extra bytes are overwritten by the match that follows.
    ldr     q0, [x19, x7]
    str     q0, [x21, x24]

    add     x13, x21, x16               // where the match is written
    sub     x15, x13, x11               // and where it is read from
    cmp     x11, #32
    b.lo    .Lfast_near

    // Far enough back that source and destination cannot overlap within one
    // block, so the widest move available is also the correct one.
    ldp     q0, q1, [x15]
    stp     q0, q1, [x13]

    mov     x24, x12
    add     x23, x8, #2
    b       .Lfast

.Lfast_near:
    mov     x0, x13
    mov     x1, x15
    mov     x2, x11
    mov     x3, x14
    bl      .Lrepeat
    mov     x24, x12
    add     x23, x8, #2
    b       .Lfast

    /*
     * Outside the zone: the same format, every question asked again.
     *
     * Reached once per value, when either cursor nears its buffer. It does not
     * hand back -- the general writer below returns to `.Lfast`, which re-tests
     * the zone and arrives here again. One test rather than two paths.
     */
.Ltail:
    cmp     x24, x22
    b.hs    .Ldone
    cmp     x23, x20
    b.hs    .Lfail
    ldrb    w4, [x19, x23]              // token
    lsr     w5, w4, #4
    and     w26, w4, #15

    /* ---- one block, whatever its lengths ----------------------------- */
.Lslow:
    add     x23, x23, #1

    mov     x15, x5                     // literal length
    cmp     x5, #15
    b.ne    .Lslow_lit_ready
    /*
     * The extended length, read here rather than called.
     *
     * On data that compresses well this runs once per block, and a call for it
     * is a branch out, a branch back and a return-stack slot to move at most a
     * couple of bytes. liblz4 has it as a self-loop inside its decode loop for
     * the same reason.
     */
    mov     x8, #0
.Llitlen_byte:
    cmp     x23, x20
    b.hs    .Lfail
    ldrb    w9, [x19, x23]
    add     x23, x23, #1
    add     x8, x8, x9
    cmp     w9, #255
    b.ne    .Llitlen_done
    // A run of 255s on a corrupt stream would otherwise spin until it wrapped.
    mov     x10, #0x20000000            // MAX_UNPACKED, 512 MiB
    cmp     x8, x10
    b.hi    .Lfail
    b       .Llitlen_byte
.Llitlen_done:
    add     x15, x15, x8

.Lslow_lit_ready:
    add     x16, x23, x15               // one past the literals
    cmp     x16, x20
    b.hi    .Lfail                      // the run runs off the input
    add     x17, x24, x15
    cmp     x17, x22
    b.hi    .Lfail                      // more output than was declared

    cbz     x15, .Lslow_lit_done

    // Wide while a whole block is readable from the input, exact for the tail:
    // overrunning the input would read memory that is not ours. The output side
    // needs no test -- see the header.
    add     x9, x16, #32
    cmp     x9, x20
    add     x0, x21, x24
    add     x1, x19, x23
    mov     x2, x15
    b.hi    .Lslow_lit_exact
    mov     x5, #0
.Llitcp_block:
    ldp     q0, q1, [x1]
    stp     q0, q1, [x0]
    add     x5, x5, #32
    cmp     x5, x2
    b.hs    .Llitcp_out
    add     x1, x1, #32
    add     x0, x0, #32
    b       .Llitcp_block
.Llitcp_out:
    b       .Lslow_lit_done
.Lslow_lit_exact:
    bl      .Lexact

.Lslow_lit_done:
    mov     x24, x17
    mov     x23, x16

    cmp     x24, x22
    b.hs    .Ldone                      // the value ends on a literal run

    add     x9, x23, #2
    cmp     x9, x20
    b.hi    .Lfail
    ldrh    w11, [x19, x23]             // offset, kept clear of the length reader
    mov     x23, x9

    mov     x14, x26                    // match length, less MIN_MATCH
    cmp     x26, #15
    b.ne    .Lslow_mat_ready
    /*
     * The extended length, read here rather than called.
     *
     * On data that compresses well this runs once per block, and a call for it
     * is a branch out, a branch back and a return-stack slot to move at most a
     * couple of bytes. liblz4 has it as a self-loop inside its decode loop for
     * the same reason.
     */
    mov     x8, #0
.Lmatlen_byte:
    cmp     x23, x20
    b.hs    .Lfail
    ldrb    w9, [x19, x23]
    add     x23, x23, #1
    add     x8, x8, x9
    cmp     w9, #255
    b.ne    .Lmatlen_done
    // A run of 255s on a corrupt stream would otherwise spin until it wrapped.
    mov     x10, #0x20000000            // MAX_UNPACKED, 512 MiB
    cmp     x8, x10
    b.hi    .Lfail
    b       .Lmatlen_byte
.Lmatlen_done:
    add     x14, x14, x8
.Lslow_mat_ready:
    add     x14, x14, #4

    cbz     w11, .Lfail
    cmp     x11, x24
    b.hi    .Lfail
    add     x12, x24, x14
    cmp     x12, x22
    b.hi    .Lfail

    add     x13, x21, x24
    sub     x15, x13, x11
    cmp     x11, #32
    b.lo    .Lslow_near

    mov     x2, x14
    mov     x5, #0
.Lmatcp_block:
    ldp     q0, q1, [x15]
    stp     q0, q1, [x13]
    add     x5, x5, #32
    cmp     x5, x2
    b.hs    .Lmatcp_out
    add     x15, x15, #32
    add     x13, x13, #32
    b       .Lmatcp_block
.Lmatcp_out:
    mov     x24, x12
    b       .Lfast

.Lslow_near:
    mov     x0, x13
    mov     x1, x15
    mov     x2, x11
    mov     x3, x14
    bl      .Lrepeat
    mov     x24, x12
    b       .Lfast

.Ldone:
    cmp     x24, x22
    b.ne    .Lfail
    mov     w0, w22
    b       .Lreturn

.Lfail:
    mov     w0, wzr
.Lreturn:
    ldp     x27, x28, [sp, #80]
    ldp     x25, x26, [sp, #64]
    ldp     x23, x24, [sp, #48]
    ldp     x21, x22, [sp, #32]
    ldp     x19, x20, [sp, #16]
    ldp     x29, x30, [sp], #96
    ret

/*
 * Copy exactly `x2` bytes from `x1` to `x0`, writing nothing beyond them.
 *
 * For the tail of the input, where a block move would read past the buffer, and
 * for the doubling steps below, where writing beyond a step would clobber the
 * bytes the next step is about to read. Widest first, then one rung per
 * remaining bit. A leaf. Clobbers x0, x1, x2, x4, x6, v0, v1.
 */
.Lexact:
    mov     x6, x2
.Lex_32:
    cmp     x6, #32
    b.lo    .Lex_16
    ldp     q0, q1, [x1]
    stp     q0, q1, [x0]
    add     x1, x1, #32
    add     x0, x0, #32
    sub     x6, x6, #32
    b       .Lex_32
.Lex_16:
    tbz     x6, #4, .Lex_8
    ldr     q0, [x1], #16
    str     q0, [x0], #16
.Lex_8:
    tbz     x6, #3, .Lex_4
    ldr     x4, [x1], #8
    str     x4, [x0], #8
.Lex_4:
    tbz     x6, #2, .Lex_2
    ldr     w4, [x1], #4
    str     w4, [x0], #4
.Lex_2:
    tbz     x6, #1, .Lex_1
    ldrh    w4, [x1], #2
    strh    w4, [x0], #2
.Lex_1:
    tbz     x6, #0, .Lex_ret
    ldrb    w4, [x1]
    strb    w4, [x0]
.Lex_ret:
    ret

/*
 * Lay down a repeat whose source is less than a block behind its destination.
 *
 *   x0 destination, x1 source, x2 offset, x3 length
 *
 * A block move would read bytes it has not written yet, so the pattern goes
 * down in a doubling sequence instead: copy `offset` bytes, then twice that,
 * then twice again. Each step reads only what earlier steps have written, and
 * each step's source ends exactly where its destination begins, so no step
 * overlaps itself.
 *
 * An offset of one gets its own path. That is a run of a single repeated value
 * -- padding, zero fill, long spaces -- and `dup` produces sixteen copies of it
 * in one instruction where the doubling would take four rounds to get there.
 *
 * Not a leaf: it calls .Lexact, so x30 is saved. Clobbers x0-x7, v0, v1, and
 * relies on .Lexact leaving x3, x5 and x7 alone.
 */
.Lrepeat:
    stp     x29, x30, [sp, #-32]!
    mov     x29, sp
    stp     x0, x1, [sp, #16]           // destination base and source

    cmp     x2, #1
    b.ne    .Lrep_double

    ld1r    {v0.16b}, [x1]
    mov     x5, #0
.Lrep_splat:
    str     q0, [x0, x5]
    add     x5, x5, #16
    cmp     x5, x3
    b.lo    .Lrep_splat
    b       .Lrep_ret

.Lrep_double:
    mov     x7, x2                      // this step's width
    mov     x5, #0                      // written so far
.Lrep_step:
    sub     x6, x3, x5                  // still owed
    cmp     x7, x6
    csel    x6, x7, x6, lo              // take = min(width, owed)
    ldp     x0, x1, [sp, #16]
    add     x0, x0, x5
    mov     x2, x6
    // Advanced before the call, because .Lexact is free to destroy x2 and x6.
    add     x5, x5, x6
    lsl     x7, x7, #1
    bl      .Lexact
    cmp     x5, x3
    b.lo    .Lrep_step

.Lrep_ret:
    ldp     x29, x30, [sp], #32
    ret

#if !defined(__APPLE__)
    .size KEVA_SYM(keva_unpack), . - KEVA_SYM(keva_unpack)
    .section .note.GNU-stack, "", %progbits
#endif
