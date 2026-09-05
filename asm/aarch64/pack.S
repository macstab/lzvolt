/*
 * keva_pack -- the whole packer, one call per value.
 *
 *   uint32_t keva_pack(const uint8_t *src, uintptr_t src_len,
 *                      uint8_t *dst, uintptr_t dst_cap,
 *                      uint32_t *table);
 *
 * Returns bytes written, or 0 when packing did not help and the caller should
 * store the original.
 *
 * AAPCS64: x0 src, x1 src_len, x2 dst, x3 dst_cap, x4 table. Result in w0.
 *
 * # Why the whole thing and not the search alone
 *
 * The search was written by itself first and measured 4-17% ahead of the
 * compiler's version. Wired in at one call per match it made the packer 16-29%
 * *slower*: an `extern "C"` boundary is a handful of cycles, but it stops the
 * compiler carrying loop state in registers, and crossing it once per match
 * costs more than the kernel wins. The fix is not a faster boundary, it is
 * fewer of them — so this crosses once per value and keeps everything in
 * registers in between.
 *
 * # Output format, matching keva_core::store::pack
 *
 *   [original length: varint][block][block]...
 *   block := [token][extended literal length][literals][offset u16][extended match length]
 *            token high nibble = literal count, 15 means "read more"
 *            token low nibble  = match length - 4, 15 means "read more"
 *
 * # Bail-out rather than bounds checks
 *
 * The moment the output reaches the input length, packing has failed to help
 * and the result would be discarded anyway. Returning 0 there is both the
 * answer the caller wants and what keeps every store below `dst_cap`, provided
 * the caller passes `dst_cap >= src_len + 16` — checked on entry.
 *
 * Constants are duplicated from pack.rs because a `.S` file cannot read Rust
 * consts. `the_duplicated_constants_still_hold` fails if they drift.
 *
 *   HASH_BITS 12 -> shift 20, or 11 -> 21 above 8192 bytes; see .Lshift_ready
 *   MAX_OFFSET 65535   MIN_MATCH 4   SKIP_TRIGGER 6   EMPTY 0x80000000
 */

#include "../common.h"

    .text
    .p2align 4
    .globl KEVA_SYM(keva_pack)

KEVA_SYM(keva_pack):
    // x30 is saved because this function calls out to the block writer; without
    // it the first `bl` overwrites the caller's return address and the final
    // `ret` jumps into whatever was left there.
    stp     x29, x30, [sp, #-96]!
    stp     x19, x20, [sp, #16]
    stp     x21, x22, [sp, #32]
    stp     x23, x24, [sp, #48]
    stp     x25, x26, [sp, #64]
    stp     x27, x28, [sp, #80]
    mov     x29, sp

    mov     x19, x0                     // src
    mov     x20, x1                     // src_len
    mov     x21, x2                     // dst
    mov     x22, x4                     // table

    // dst_cap must leave room for the bail-out check to be sufficient.
    add     x9, x20, #16
    cmp     x3, x9
    b.lo    .Lgiveup

    cmp     x20, #4                     // shorter than MIN_MATCH cannot pack
    b.lo    .Lgiveup

    mov     x23, #0                     // out cursor
    mov     x24, #0                     // literal_start
    mov     x25, #0                     // at
    mov     x26, #64                    // misses = 1 << SKIP_TRIGGER

    /* ---- header: LEB128 of src_len ---------------------------------- */
    mov     x9, x20
.Lvarint:
    and     w10, w9, #0x7F
    lsr     x9, x9, #7
    cbz     x9, .Lvarint_last
    orr     w10, w10, #0x80
    strb    w10, [x21, x23]
    add     x23, x23, #1
    b       .Lvarint
.Lvarint_last:
    strb    w10, [x21, x23]
    add     x23, x23, #1

    sub     x8, x20, #4                 // scan limit
    mov     w7, #0x79B1                 // 0x9E3779B1
    movk    w7, #0x9E37, lsl #16
    mov     w13, #65534                 // MAX_OFFSET - 1; see the window check

    /*
     * How much of the table this value gets to use, chosen once from its length.
     *
     * The whole table is always allocated; the shift only decides how much of it
     * a value touches, and touched lines are what the cache charges for. Below
     * the threshold a value and the full table sit in L1 together and the wider
     * table wins on collisions -- 4 KiB of noise packs at 4.47 GiB/s with twelve
     * bits and 3.74 with eleven, because every collision there is a verify load
     * to a random address that cannot possibly succeed. Above it the table is
     * displacing the data instead, and the relationship inverts: a 64 KiB value
     * packs at 0.61 with twelve bits and 0.87 with eleven.
     *
     * A register shift costs the same as an immediate one, so this is free.
     */
    mov     w27, #20                    // 32 - HASH_BITS
    mov     x9, #8192                   // NARROW_TABLE_ABOVE
    cmp     x20, x9
    b.ls    .Lshift_ready
    mov     w27, #21                    // 32 - HASH_BITS_LARGE
.Lshift_ready:

    /* ---- search ------------------------------------------------------ */

.Lscan:
    cmp     x25, x8
    b.hi    .Ltail

    ldr     w9, [x19, x25]              // four bytes at `at`
    mul     w10, w9, w7
    lsr     w10, w10, w27               // slot

    ldr     w11, [x22, x10, lsl #2]     // stored
    add     w12, w25, #1
    str     w12, [x22, x10, lsl #2]     // table[slot] = at + 1

    /*
     * Every rejection in one unsigned compare, including the empty slot.
     *
     * `stored` is already candidate + 1, so subtracting it from the cursor
     * gives `at - candidate - 1` without materialising the candidate first.
     * Three things have to be rejected and all three fail this one test:
     *
     *   - an offset past the window          -- the difference exceeds the bound
     *   - a candidate at or ahead of the cursor, which a stale entry left by an
     *     earlier call can be                -- the subtraction wraps
     *   - a slot never written               -- it holds KEVA_PACK_EMPTY, which
     *                                           is large enough to wrap too
     *
     * That last one is why the caller must fill the table with the sentinel
     * rather than with zero: a zero slot would read as position -1 and send the
     * verify load outside the buffer. `pack_find::new_table` is the only
     * supported way to build one.
     *
     * The bound is MAX_OFFSET - 1 because the subtraction already took one off.
     */
    sub     w12, w25, w11
    cmp     w12, w13
    b.hi    .Lmiss

    sub     w11, w11, #1                // candidate, now that it is worth having
    ldr     w14, [x19, x11]
    cmp     w14, w9
    b.ne    .Lmiss                      // the hash collided

    /* ---- extend forward, eight bytes at a time ----------------------- */
    add     x15, x25, #4
    add     x16, x11, #4
    mov     x17, #4                     // matched

.Lext8:
    add     x9, x15, #8
    cmp     x9, x20
    b.hi    .Lext1

    ldr     x9, [x19, x15]
    ldr     x10, [x19, x16]
    eor     x9, x9, x10
    cbnz    x9, .Ldiff

    add     x15, x15, #8
    add     x16, x16, #8
    add     x17, x17, #8
    b       .Lext8

.Ldiff:
    rbit    x9, x9
    clz     x9, x9
    lsr     x9, x9, #3                  // bit index -> byte index
    add     x17, x17, x9
    b       .Lemit

.Lext1:
    cmp     x15, x20
    b.hs    .Lemit
    ldrb    w9, [x19, x15]
    ldrb    w10, [x19, x16]
    cmp     w9, w10
    b.ne    .Lemit
    add     x15, x15, #1
    add     x16, x16, #1
    add     x17, x17, #1
    b       .Lext1

    /* ---- extend backward into the pending literals ------------------- */
.Lemit:
    mov     x0, #0                      // back
.Lback:
    cmp     x11, x0
    b.ls    .Lback_done                 // candidate would go below zero
    sub     x9, x25, x0
    cmp     x9, x24
    b.ls    .Lback_done                 // would eat into already-emitted output
    sub     x9, x11, x0
    sub     x9, x9, #1
    ldrb    w9, [x19, x9]
    sub     x10, x25, x0
    sub     x10, x10, #1
    ldrb    w10, [x19, x10]
    cmp     w9, w10
    b.ne    .Lback_done
    add     x0, x0, #1
    b       .Lback
.Lback_done:
    sub     x1, x25, x0                 // literal end
    add     x2, x17, x0                 // match length including the back part
    sub     x3, x1, x24                 // literal count
    sub     x4, x25, x11                // offset

    bl      .Lblock
    cbz     x0, .Lgiveup_restore        // block writer says the output grew too far

    add     x25, x25, x17               // at += matched (forward part only)
    mov     x24, x25                    // literal_start = at
    mov     x26, #64                    // misses reset
    b       .Lscan

.Lmiss:
    lsr     x9, x26, #6
    add     x25, x25, x9
    add     x26, x26, #1
    b       .Lscan

    /* ---- trailing literals, no match follows ------------------------- */
.Ltail:
    sub     x3, x20, x24                // literal count
    mov     x1, x20                     // literal end
    mov     x2, #0                      // no match
    mov     x4, #0
    bl      .Lblock
    cbz     x0, .Lgiveup_restore

    cmp     x23, x20
    b.hs    .Lgiveup_restore            // did not actually shrink
    mov     w0, w23
    b       .Lreturn

.Lgiveup_restore:
    mov     w0, wzr
.Lreturn:
    ldp     x27, x28, [sp, #80]
    ldp     x25, x26, [sp, #64]
    ldp     x23, x24, [sp, #48]
    ldp     x21, x22, [sp, #32]
    ldp     x19, x20, [sp, #16]
    ldp     x29, x30, [sp], #96
    ret

.Lgiveup:
    mov     w0, wzr
    b       .Lreturn

/*
 * Write one block.
 *
 *   x1 = end of the literal run, x2 = match length (0 for the tail),
 *   x3 = literal count, x4 = offset.
 *   Returns x0 = 1 on success, 0 when the output has reached src_len.
 *
 * Clobbers x5, x6, x9, x10, x12, v0, v1 and advances x23. x13 is left alone
 * so the search can keep a loop-invariant constant there.
 *
 * A leaf: it calls nothing, so x30 survives untouched and there is no frame to
 * build. Saving one here cost two memory accesses per match for nothing.
 */
.Lblock:
    // Refuse once the output has reached the input length: packing has failed
    // to help by then and the result would be discarded anyway.
    //
    // This doubles as the bound on every store below. Past this point a block
    // writes at most `literal_count + 13` bytes — one token, five extended
    // length bytes, the literals, a two-byte offset and five more extended
    // bytes — so the cursor stays under `src_len + 13`, which is why a caller
    // buffer of `src_len + 16` needs no further checking.
    add     x9, x23, x3
    cmp     x9, x20
    b.hs    .Lblock_full

    mov     x5, #0                      // match_extra
    cbz     x2, .Lblock_token
    sub     x5, x2, #4

.Lblock_token:
    mov     x9, #15
    cmp     x3, #15
    csel    x10, x3, x9, lo             // literal nibble
    cmp     x5, #15
    csel    x12, x5, x9, lo             // match nibble
    cbz     x2, .Lblock_no_match_nibble
    b       .Lblock_have_nibbles
.Lblock_no_match_nibble:
    mov     x12, #0
.Lblock_have_nibbles:
    lsl     w10, w10, #4
    orr     w10, w10, w12
    strb    w10, [x21, x23]
    add     x23, x23, #1

    // Extended literal length.
    cmp     x3, #15
    b.lo    .Lblock_literals
    sub     x9, x3, #15
.Lblock_lit_ext:
    cmp     x9, #255
    b.lo    .Lblock_lit_ext_last
    mov     w10, #255
    strb    w10, [x21, x23]
    add     x23, x23, #1
    sub     x9, x9, #255
    b       .Lblock_lit_ext
.Lblock_lit_ext_last:
    strb    w9, [x21, x23]
    add     x23, x23, #1

/*
 * Copy the literals, 32 bytes at a time.
 *
 * This ran a byte at a time first and it was the wrong thing to be doing: seven
 * instructions, a loop-carried dependency and a branch, per byte, against the
 * Rust side's `extend_from_slice` -- which is a vectorised memcpy. On data that
 * matches rarely almost every byte comes through here, so that loop was most of
 * what the packer did.
 *
 * `remaining >= 32` is the only guard needed, and it bounds both sides at once.
 * Source: `remaining` counts bytes still inside the literal run, so a 32-byte
 * read ends at or before the run's end, which is at or before `src_len`.
 * Destination: the block writer already refused unless `cursor + count <
 * src_len`, and a 32-byte store ends at or before `cursor + count`, so it stays
 * inside the caller's `src_len + 16`. Each rung of the tail ladder is the same
 * argument with a smaller width.
 *
 * v0 and v1 are caller-saved, so the wide path costs no spill.
 */
.Lblock_literals:
    cbz     x3, .Lblock_offset
    sub     x6, x1, x3                  // literal start, as an index
    add     x9, x19, x6                 // read pointer
    add     x10, x21, x23               // write pointer
    add     x23, x23, x3                // the cursor lands here either way

    /*
     * One vector move for a short run, and most runs are short.
     *
     * The destination always has room: the block writer refused unless `cursor
     * + count < src_len`, and the caller's buffer is `src_len + 16`, so sixteen
     * bytes fit whatever the count. Only the source needs asking about, and
     * only within sixteen bytes of the end of the value.
     *
     * Sixteen unconditionally, not the narrowest width that fits. Choosing
     * between four, eight and sixteen was tried and measured worse on five of
     * six shapes -- the two extra compares cost more than the store traffic
     * they save, even where the runs are one to three bytes long. An
     * unconditional wide move is cheaper than any decision to avoid it.
     */
    cmp     x3, #16
    b.hi    .Lblock_copy_long
    add     x12, x6, #16
    cmp     x12, x20
    b.hi    .Lblock_copy_short_exact
    ldr     q0, [x9]
    str     q0, [x10]
    b       .Lblock_offset

.Lblock_copy_short_exact:
    mov     x6, x3
    b       .Lblock_copy_exact

.Lblock_copy_long:
    mov     x6, x3                      // remaining; the start is in x9 now
.Lblock_copy32:
    cmp     x6, #32
    b.lo    .Lblock_copy_exact
    ldp     q0, q1, [x9]
    stp     q0, q1, [x10]
    add     x9, x9, #32
    add     x10, x10, #32
    sub     x6, x6, #32
    b       .Lblock_copy32

/*
 * The exact tail: two overlapping moves, never a bit at a time.
 *
 * A run of six bytes is a four-byte move from the front and another from four
 * back; the middle two are written twice and the result is exact. The ladder
 * this replaces walked one conditional branch per bit of the length -- five of
 * them to move those same six bytes, with the same two loads and two stores at
 * the end of it. The profile put that ladder at 15.7% of the packer, and it was
 * never the copying, it was deciding how wide to copy.
 *
 * Nothing here reads or writes outside the run, so it is also what serves the
 * end of the value, where overshooting the source is not allowed.
 *
 * The scratch is x6, not x5: x5 carries the match length across this routine
 * and is read again once the literals are down. The differential test caught
 * that within a second, on a seven-byte value.
 */
.Lblock_copy_exact:
    cbz     x6, .Lblock_offset
    cmp     x6, #16
    b.lo    .Lblock_copy_under16
    sub     x12, x6, #16
    ldr     q0, [x9]
    ldr     q1, [x9, x12]
    str     q0, [x10]
    str     q1, [x10, x12]
    b       .Lblock_offset

.Lblock_copy_under16:
    cmp     x6, #8
    b.lo    .Lblock_copy_under8
    sub     x12, x6, #8
    ldr     x6, [x9]
    str     x6, [x10]
    ldr     x6, [x9, x12]
    str     x6, [x10, x12]
    b       .Lblock_offset

.Lblock_copy_under8:
    cmp     x6, #4
    b.lo    .Lblock_copy_under4
    sub     x12, x6, #4
    ldr     w6, [x9]
    str     w6, [x10]
    ldr     w6, [x9, x12]
    str     w6, [x10, x12]
    b       .Lblock_offset

.Lblock_copy_under4:
    cmp     x6, #2
    b.lo    .Lblock_copy_one
    sub     x12, x6, #2
    ldrh    w6, [x9]
    strh    w6, [x10]
    ldrh    w6, [x9, x12]
    strh    w6, [x10, x12]
    b       .Lblock_offset

.Lblock_copy_one:
    ldrb    w6, [x9]
    strb    w6, [x10]

.Lblock_offset:
    cbz     x2, .Lblock_done            // the tail carries no match

    strb    w4, [x21, x23]
    add     x23, x23, #1
    lsr     w9, w4, #8
    strb    w9, [x21, x23]
    add     x23, x23, #1

    cmp     x5, #15
    b.lo    .Lblock_done
    sub     x9, x5, #15
.Lblock_match_ext:
    cmp     x9, #255
    b.lo    .Lblock_match_ext_last
    mov     w10, #255
    strb    w10, [x21, x23]
    add     x23, x23, #1
    sub     x9, x9, #255
    b       .Lblock_match_ext
.Lblock_match_ext_last:
    strb    w9, [x21, x23]
    add     x23, x23, #1

.Lblock_done:
    mov     x0, #1
    ret

.Lblock_full:
    mov     x0, #0
    ret

#if !defined(__APPLE__)
    .size KEVA_SYM(keva_pack), . - KEVA_SYM(keva_pack)
    .section .note.GNU-stack, "", %progbits
#endif
