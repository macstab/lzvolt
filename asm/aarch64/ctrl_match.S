/*
 * keva_ctrl_match_neon -- scan one 16-slot control group for a tag byte.
 *
 *   uint64_t keva_ctrl_match_neon(const uint8_t *ctrl, uint8_t tag);
 *
 * AAPCS64: x0 = ctrl, w1 = tag, result in x0.
 *
 * Returns a *nibble* mask: four bits per slot, so slot i occupies bits
 * [4i, 4i+3]. AArch64 has no `pmovmskb`, and the usual workarounds (AND with a
 * bit-weight vector then `addv`, or a `zip`/`shl` chain) cost several extra
 * instructions plus a constant load. Narrowing with `shrn` carries the same
 * information in one instruction, and the caller divides the trailing-zero
 * count by four. That is why `BitMask` in lib.rs carries a stride rather than
 * always being a dense u16: the representation follows the cheapest sequence on
 * each target instead of being forced into a common shape.
 *
 * How the narrowing works: after `cmeq` each byte lane is 0x00 or 0xFF. Viewed
 * as eight 16-bit lanes, each holds a pair of slots and is one of 0x0000,
 * 0x00FF, 0xFF00, 0xFFFF. `shrn #4` shifts right by four and keeps the low byte
 * of each, giving 0x00, 0x0F, 0xF0, 0xFF -- the low nibble is the even slot,
 * the high nibble the odd one.
 *
 * The load is unaligned on purpose. Groups are 16-byte aligned in the table
 * today, but probing has to wrap past the end of a group, and `ld1` handles
 * unaligned addresses at no cost on every ARMv8 core we target.
 */

#include "../common.h"

    .text
    .p2align 4
    .globl KEVA_SYM(keva_ctrl_match_neon)

KEVA_SYM(keva_ctrl_match_neon):
    ld1     {v0.16b}, [x0]              // 16 control bytes
    dup     v1.16b, w1                  // broadcast the tag
    cmeq    v0.16b, v0.16b, v1.16b      // 0xFF in every matching lane
    shrn    v0.8b, v0.8h, #4            // 0xFF/0x00 pairs -> nibble per slot
    fmov    x0, d0
    ret

#if !defined(__APPLE__)
    .size KEVA_SYM(keva_ctrl_match_neon), . - KEVA_SYM(keva_ctrl_match_neon)
    .section .note.GNU-stack, "", %progbits
#endif
