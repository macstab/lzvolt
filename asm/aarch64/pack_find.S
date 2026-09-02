/*
 * keva_pack_find -- the packer's match search, one call per match.
 *
 *   void keva_pack_find(const uint8_t *input, uintptr_t len,
 *                       uint32_t *table, KevaPackState *state);
 *
 *   struct KevaPackState {
 *       uint32_t at;         // in: where to resume; out: where the match sits
 *       uint32_t candidate;  // out: what it matched against
 *       uint32_t len;        // out: match length, 0 when the scan ran out
 *       uint32_t misses;     // in/out: the skip accelerator's counter
 *   };
 *
 * AAPCS64: x0 = input, x1 = len, x2 = table, x3 = state.
 * Uses x4-x17 only, all caller-saved, so nothing is spilled or restored.
 *
 * This is the loop the Rust profile puts every sample inside. Per position it
 * hashes four bytes, reads and rewrites one table slot, and — only when the
 * slot holds something plausible — compares four bytes. Almost every position
 * ends at that comparison, which is why it comes before anything expensive.
 *
 * The constants are duplicated from pack.rs rather than shared, because a `.S`
 * file cannot see Rust consts. `pack_find_agrees_with_rust` in lib.rs fails
 * loudly if they drift, which is the only reason duplicating them is
 * acceptable.
 *
 *   HASH_BITS    12   ->  shift the 32-bit product right by 20
 *   MAX_OFFSET   65535
 *   MIN_MATCH    4
 *   SKIP_TRIGGER 6
 */

#include "../common.h"

    .text
    .p2align 4
    .globl KEVA_SYM(keva_pack_find)

KEVA_SYM(keva_pack_find):
    ldr     w4, [x3]                    // at
    ldr     w5, [x3, #12]               // misses

    subs    x6, x1, #4                  // limit = len - MIN_MATCH
    b.lo    .Lnone                      // input shorter than a match

    mov     w7, #0x79B1                 // the mixing constant, 0x9E3779B1
    movk    w7, #0x9E37, lsl #16

.Lscan:
    cmp     x4, x6
    b.hi    .Lnone                      // at + MIN_MATCH > len

    ldr     w8, [x0, x4]                // four bytes at `at`
    mul     w9, w8, w7
    lsr     w9, w9, #20                 // slot = hash >> (32 - HASH_BITS)

    ldr     w10, [x2, x9, lsl #2]       // stored = table[slot]
    add     w11, w4, #1
    str     w11, [x2, x9, lsl #2]       // table[slot] = at + 1

    cbz     w10, .Lmiss                 // zero means the slot was empty
    sub     w10, w10, #1                // candidate = stored - 1

    cmp     w10, w4
    b.hs    .Lmiss                      // candidate must be behind the cursor

    sub     w12, w4, w10
    mov     w13, #65535
    cmp     w12, w13
    b.hi    .Lmiss                      // offset beyond the window

    ldr     w14, [x0, x10]              // four bytes at the candidate
    cmp     w14, w8
    b.ne    .Lmiss                      // the hash collided; nothing here

    /*
     * A real match. Extend it eight bytes at a time from just past the four
     * already known equal. The XOR of two differing words has its first
     * differing byte at the lowest set bit; `rbit` then `clz` is how AArch64
     * counts trailing zeros, and dividing by eight turns bits into bytes.
     */
    add     x15, x4, #4                 // scan position
    add     x16, x10, #4                // candidate position
    mov     w17, #4                     // matched so far

.Lextend8:
    add     x11, x15, #8
    cmp     x11, x1
    b.hi    .Lextend1                   // fewer than eight bytes left

    ldr     x11, [x0, x15]
    ldr     x12, [x0, x16]
    eor     x11, x11, x12
    cbnz    x11, .Ldiffer

    add     x15, x15, #8
    add     x16, x16, #8
    add     w17, w17, #8
    b       .Lextend8

.Ldiffer:
    rbit    x11, x11
    clz     x11, x11                    // bit index of the first difference
    lsr     w11, w11, #3                // -> byte index
    add     w17, w17, w11
    b       .Lfound

.Lextend1:
    cmp     x15, x1
    b.hs    .Lfound
    ldrb    w11, [x0, x15]
    ldrb    w12, [x0, x16]
    cmp     w11, w12
    b.ne    .Lfound
    add     x15, x15, #1
    add     x16, x16, #1
    add     w17, w17, #1
    b       .Lextend1

.Lfound:
    str     w4, [x3]                    // at
    str     w10, [x3, #4]               // candidate
    str     w17, [x3, #8]               // len
    mov     w5, #64                     // misses = 1 << SKIP_TRIGGER
    str     w5, [x3, #12]
    ret

.Lmiss:
    lsr     w11, w5, #6                 // step = misses >> SKIP_TRIGGER
    add     w4, w4, w11
    add     w5, w5, #1
    b       .Lscan

.Lnone:
    str     w4, [x3]                    // where the scan stopped
    str     wzr, [x3, #8]               // len = 0 means nothing found
    str     w5, [x3, #12]
    ret

#if !defined(__APPLE__)
    .size KEVA_SYM(keva_pack_find), . - KEVA_SYM(keva_pack_find)
    .section .note.GNU-stack, "", %progbits
#endif
