/*
 * keva_ctrl_match_sse2 -- scan one 16-slot control group for a tag byte.
 *
 *   uint32_t keva_ctrl_match_sse2(const uint8_t *ctrl, uint8_t tag);
 *
 * System V AMD64: rdi = ctrl, esi = tag, result in eax.
 *
 * Returns a dense 16-bit mask, one bit per slot -- `pmovmskb` produces exactly
 * that, so unlike the AArch64 kernel there is no reason to use a wider stride.
 *
 * SSE2 rather than AVX2 on purpose: a 16-slot group is precisely one XMM
 * register, so a wider vector has nothing extra to do here, and SSE2 is part of
 * the x86-64 baseline -- this kernel needs no runtime dispatch and no branch in
 * front of the call. AVX2 and AVX-512 earn their place on the streaming kernels
 * (expiry sweep, eviction sampling) where there is real width to exploit; see
 * docs/DESIGN_DECISION_ASM_KERNELS.md.
 *
 * This is the System V register assignment. A Windows build would take the
 * pointer in rcx and the tag in edx; we do not target Windows yet.
 */

#include "../common.h"

    .text
    .p2align 4
    .globl KEVA_SYM(keva_ctrl_match_sse2)

KEVA_SYM(keva_ctrl_match_sse2):
    movd        %esi, %xmm1             /* tag into the low byte */
    punpcklbw   %xmm1, %xmm1            /* broadcast it across all 16 lanes */
    punpcklwd   %xmm1, %xmm1
    pshufd      $0, %xmm1, %xmm1
    movdqu      (%rdi), %xmm0           /* 16 control bytes */
    pcmpeqb     %xmm1, %xmm0            /* 0xFF in every matching lane */
    pmovmskb    %xmm0, %eax             /* one bit per lane */
    ret

#if !defined(__APPLE__)
    .size KEVA_SYM(keva_ctrl_match_sse2), . - KEVA_SYM(keva_ctrl_match_sse2)
    .section .note.GNU-stack, "", %progbits
#endif
