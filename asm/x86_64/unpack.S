/*
 * keva_unpack -- the decoder, one call per value. x86-64 counterpart.
 *
 *   uint32_t keva_unpack(const uint8_t *src, uintptr_t src_len,
 *                        uint8_t *dst, uintptr_t dst_cap,
 *                        uintptr_t declared, uintptr_t start);
 *
 * System V AMD64: rdi src, rsi src_len, rdx dst, rcx dst_cap, r8 declared,
 * r9 start. Result in eax.
 *
 * The contract is the AArch64 kernel's, unchanged, and both are diffed against
 * the same portable decoder. `start` is where in `dst` this call begins
 * producing; a value whose token split changes partway is decoded by two calls
 * sharing one destination, so the second section's backward references reach
 * into what the first wrote. Returns `declared` on success, or 0 meaning "this
 * needs the portable decoder" -- for a malformed stream, or a buffer too small
 * for the checks below to be sufficient. The caller then runs the Rust decoder,
 * which produces the precise error, so every message stays in one place.
 *
 * SSE2 only, no runtime dispatch: 16-byte moves are all this needs and SSE2 is
 * the x86-64 baseline, so nothing tests for a feature in front of the call.
 *
 * # The same work per block as the AArch64 kernel, with one exception
 *
 * Both limits the fast path tests are loop-invariant, and AArch64 keeps both in
 * registers because it has thirty-one of them. Here one is precomputed and the
 * other costs an extra `lea` per block, which is the price of sixteen registers
 * rather than a shortcut. Everything else is the same shape -- the split-sized
 * match move, the derived output margin, the doubling splat for near matches --
 * because none of it was ever AArch64-specific.
 *
 * # What is checked, and what is checked once
 *
 * Values arrive from sockets and snapshot files, so every length here is
 * attacker-controlled and this kernel is the boundary.
 *
 * Requiring `dst_cap >= declared + 64` on entry is what lets the exact path
 * carry no capacity test: `produced + len <= declared` is checked before every
 * copy, and a copy rounded up to whole blocks then ends by
 * `declared + OVERRUN - 1 < dst_cap`. The fast path can write more than that at
 * once under the wide split, which is what OUT_MARGIN is for.
 *
 * The input side is checked per block, because `src_len` bounds nothing about
 * how far a chain of length bytes can run.
 *
 * # Measurement
 *
 * Written on Apple silicon, where x86 runs only under a binary translator.
 * Correctness is verified there against the portable decoder -- byte for byte,
 * over every truncation and every single-bit flip -- but the timing of a
 * translated instruction stream means nothing, so nothing here rests on a
 * measurement taken on this architecture. What it rests on is the AArch64
 * kernel and the portable decoder, where each of these shapes was measured. The
 * x86-64 CI job runs scripts/packer-report.sh; that is where a number for this
 * file comes from.
 */

#include "../common.h"

#ifdef KEVA_WIDE
#  define KEVA_PFX   w
#  define KEVA_ENTRY keva_unpack_wide
#  define LIT_SHIFT  6
#  define LIT_MAX    3
#  define MAT_MAX    63
#else
#  define KEVA_PFX   e
#  define KEVA_ENTRY keva_unpack
#  define LIT_SHIFT  4
#  define LIT_MAX    15
#  define MAT_MAX    15
#endif

#define MIN_MATCH         4
#define OVERRUN           32
#define FAST_INPUT_SLACK  32
#define MAT_CAP           (MAT_MAX + MIN_MATCH)

/*
 * How much the fixed match move copies. A property of the hardware and not of
 * the format: one vector pair under the even split, three under the wide one,
 * because that is what covers a match the split can carry inline. Without it
 * the wide split would send every match past twenty-eight bytes to the checked
 * path -- which is the shape it exists to encode.
 */
#if MAT_CAP > 64
#  define COPY_MAX   96
#elif MAT_CAP > 32
#  define COPY_MAX   64
#else
#  define COPY_MAX   32
#endif

/*
 * How far short of the declared length the fast loop stops.
 *
 * A bound and a heuristic. The bound: the widest thing a fast block writes is
 * its literals plus COPY_MAX, and that has to land inside the 64 bytes of slack
 * the caller provides past `declared` -- so LIT_MAX + COPY_MAX - 64, which is
 * 35 under the wide split and negative, meaning no constraint, under the even
 * one.
 *
 * The heuristic: a value ends in a literal run with no offset after it, and the
 * fast path always reads an offset, so it has to stop before that last block. A
 * longer final run is not a correctness problem -- the block decodes to
 * nonsense, the length check at the end fails, and the caller runs the portable
 * decoder -- it is only slow, so this wants to be a comfortable upper bound on
 * a final run and no more.
 */
#if (LIT_MAX + COPY_MAX) > 64
#  define OUT_MARGIN (LIT_MAX + COPY_MAX - 64 + 16)
#else
#  define OUT_MARGIN 16
#endif

/* Input the fast path may read past the cursor, at its widest literal run. */
#define FAST_IN (1 + (LIT_MAX - 1) + 2 + FAST_INPUT_SLACK)

/*
 * KEVA_SYM pastes with ##, so its argument is not expanded first --
 * KEVA_SYM(KEVA_ENTRY) would name the symbol after the macro rather than after
 * what it stands for. One more level fixes it, because an argument to a macro
 * that does not paste is expanded before substitution.
 *
 * Local labels carry the split's prefix for the same reason the entry does:
 * this file is assembled twice.
 */
#define KEVA_NAME1(n) KEVA_SYM(n)
#define KEVA_NAME(n) KEVA_NAME1(n)

#define L2(p, n) .L ## p ## _ ## n
#define L1(p, n) L2(p, n)
#define L(n) L1(KEVA_PFX, n)

/*
 * Held for the whole call:
 *
 *   rbx  dst base        r13  declared      r15  src end
 *   rbp  output limit    r14  src cursor    r11  token, on the exact path
 *   r12  produced
 *
 * Everything else is scratch, which matters because `rep movsb` takes rsi, rdi
 * and rcx and returns them spent -- so nothing that has to survive a copy may
 * live there.
 */

    .text
    .p2align 4
    .globl KEVA_NAME(KEVA_ENTRY)

KEVA_NAME(KEVA_ENTRY):
    push    %rbx
    push    %rbp
    push    %r12
    push    %r13
    push    %r14
    push    %r15

    mov     %rdi, %r14                      /* src cursor */
    lea     (%rdi,%rsi,1), %r15             /* src end */
    mov     %rdx, %rbx                      /* dst base */
    mov     %r8,  %r13                      /* declared */
    mov     %r9,  %r12                      /* produced = start */

    /* The entry check the loop then never repeats. */
    lea     64(%r13), %rax
    cmp     %rax, %rcx                      /* dst_cap, still in its argument */
    jb      L(fail)

    /* Loop-invariant, so it is derived once.
     *
     * It may go negative, for a value shorter than the margin, and that case
     * wants no special handling: the comparison against it below is signed, so
     * a negative limit is simply below every position and sends every block to
     * the checked path. An unsigned comparison would wrap it to enormous and
     * send the first block down the fast path instead, which is the bug this
     * replaced. Both fit comfortably in a signed word -- MAX_UNPACKED is half a
     * gigabyte. */
    mov     %r13, %rbp
    sub     $OUT_MARGIN, %rbp

L(block):
    cmp     %r13, %r12
    jae     L(done)

    cmp     %r15, %r14
    jae     L(fail)
    movzbl  (%r14), %eax                    /* token */

    /* ---- fast path -------------------------------------------------- */

    cmp     %rbp, %r12
    jg      L(slow)                         /* too near the end to read ahead */

    lea     FAST_IN(%r14), %rcx
    cmp     %r15, %rcx
    ja      L(slow)                         /* too near the end of the stream */

    mov     %eax, %r10d
    shr     $LIT_SHIFT, %r10d               /* short literal */
    cmp     $LIT_MAX, %r10d
    je      L(slow)

    mov     %eax, %r11d
    and     $MAT_MAX, %r11d                 /* short match */
    cmp     $MAT_MAX, %r11d
    je      L(slow)

    movzwl  1(%r14,%r10,1), %ecx            /* offset */
    lea     (%r12,%r10,1), %r8              /* after_literals */

    /* Both checked before either copy: the match may reach into the literals
     * this block is about to write. */
    test    %ecx, %ecx
    je      L(fail)
    cmp     %r8, %rcx
    ja      L(fail)

    lea     MIN_MATCH(%r11), %r9            /* match length */
    lea     (%r8,%r9,1), %rax
    cmp     %r13, %rax
    ja      L(fail)

    /* Fixed width on purpose -- the guards above paid for not asking again.
     * Sixteen literal bytes are stored whether or not that many were meant;
     * the surplus is overwritten by the next block or falls in the slack the
     * caller trims. */
    movdqu  1(%r14), %xmm0
    movdqu  %xmm0, (%rbx,%r12,1)

    lea     3(%r14,%r10,1), %r14            /* token + literals + offset */

    mov     %r8, %r10
    sub     %rcx, %r10                      /* after_literals - offset */
    lea     (%rbx,%r10,1), %rsi
    lea     (%rbx,%r8,1), %rdi
    cmp     $OVERRUN, %rcx
    jb      L(near)

    movdqu  (%rsi), %xmm0
    movdqu  16(%rsi), %xmm1
    movdqu  %xmm0, (%rdi)
    movdqu  %xmm1, 16(%rdi)
#if COPY_MAX > 32
    movdqu  32(%rsi), %xmm2
    movdqu  48(%rsi), %xmm3
    movdqu  %xmm2, 32(%rdi)
    movdqu  %xmm3, 48(%rdi)
#endif
#if COPY_MAX > 64
    movdqu  64(%rsi), %xmm4
    movdqu  80(%rsi), %xmm5
    movdqu  %xmm4, 64(%rdi)
    movdqu  %xmm5, 80(%rdi)
#endif
    jmp     L(fast_end)

L(near):
    mov     %r9, %rcx
    call    L(splat)

L(fast_end):
    lea     (%r8,%r9,1), %r12               /* produced = after + match */
    jmp     L(block)

    /* ---- the pattern splat ------------------------------------------ */

/*
 * Fill `rcx` bytes at `rdi` from `rsi`, the two closer together than one block.
 *
 * A wide load here would read bytes that have not been written, so the run is
 * grown instead: what is already there is copied onto itself, doubling the
 * distance each time, until the gap is wide enough for vector moves. From an
 * offset of one that is 1 + 2 + 4 + 8 byte moves before the loop below takes
 * over -- against one byte move per output byte, which is what this replaced.
 *
 * Reached by `call` from two sites rather than written twice. Nothing else here
 * touches the stack, so the return address is all that is on it.
 */
L(splat):
    mov     %rdi, %rdx
    sub     %rsi, %rdx                      /* gap */

L(splat_grow):
    cmp     $16, %rdx
    jae     L(splat_wide)
    mov     %rdx, %r10                      /* append `gap` bytes */
    mov     %rdi, %rsi
    sub     %rdx, %rsi
L(splat_byte):
    movzbl  (%rsi), %eax
    mov     %al, (%rdi)
    inc     %rsi
    inc     %rdi
    dec     %r10
    jnz     L(splat_byte)
    sub     %rdx, %rcx
    jle     L(splat_ret)
    add     %rdx, %rdx                      /* the gap has doubled */
    jmp     L(splat_grow)

L(splat_wide):
    mov     %rdi, %rsi
    sub     %rdx, %rsi
L(splat_loop):
    movdqu  (%rsi), %xmm0
    movdqu  %xmm0, (%rdi)
    add     $16, %rsi
    add     $16, %rdi
    sub     $16, %rcx
    jg      L(splat_loop)

L(splat_ret):
    ret

    /* ---- exact path ------------------------------------------------- */

L(slow):
    inc     %r14
    mov     %eax, %r11d                     /* token, kept across both copies */

    mov     %r11d, %r10d
    shr     $LIT_SHIFT, %r10d
    cmp     $LIT_MAX, %r10d
    jne     L(lit_len)

    xor     %r10, %r10
L(lit_ext):
    cmp     %r15, %r14
    jae     L(fail)
    movzbl  (%r14), %eax
    inc     %r14
    add     %rax, %r10
    /* Bounded against `declared` rather than against overflow: nothing beyond
     * it can be produced, so a chain that reaches it has already failed. */
    cmp     %r13, %r10
    ja      L(fail)
    cmp     $255, %eax
    je      L(lit_ext)
    add     $LIT_MAX, %r10

L(lit_len):
    lea     (%r14,%r10,1), %rax
    cmp     %r15, %rax
    ja      L(fail)
    lea     (%r12,%r10,1), %r9
    cmp     %r13, %r9
    ja      L(fail)

    test    %r10, %r10
    je      L(lit_done)

    mov     %r14, %rsi
    lea     (%rbx,%r12,1), %rdi

    /* Blocks only while a whole one is readable; `rax` is the end of the run. */
    lea     OVERRUN(%rax), %r8
    cmp     %r15, %r8
    ja      L(lit_bytes)

    mov     %r10, %rcx
L(lit_wide):
    movdqu  (%rsi), %xmm0
    movdqu  16(%rsi), %xmm1
    movdqu  %xmm0, (%rdi)
    movdqu  %xmm1, 16(%rdi)
    add     $OVERRUN, %rsi
    add     $OVERRUN, %rdi
    sub     $OVERRUN, %rcx
    /* Signed: a length below one block lands at or under zero and stops. An
     * unsigned test wraps and never terminates, which the first version did. */
    jg      L(lit_wide)
    jmp     L(lit_done)

L(lit_bytes):
    mov     %r10, %rcx
    rep movsb

L(lit_done):
    lea     (%r14,%r10,1), %r14             /* at = end */
    mov     %r9, %r12                       /* produced += literals */

    cmp     %r13, %r12
    je      L(done)

    lea     2(%r14), %rax
    cmp     %r15, %rax
    ja      L(fail)
    movzwl  (%r14), %r8d                    /* offset */
    mov     %rax, %r14

    mov     %r11d, %r10d
    and     $MAT_MAX, %r10d
    cmp     $MAT_MAX, %r10d
    jne     L(mat_len)

    xor     %r10, %r10
L(mat_ext):
    cmp     %r15, %r14
    jae     L(fail)
    movzbl  (%r14), %eax
    inc     %r14
    add     %rax, %r10
    cmp     %r13, %r10
    ja      L(fail)
    cmp     $255, %eax
    je      L(mat_ext)
    add     $MAT_MAX, %r10

L(mat_len):
    add     $MIN_MATCH, %r10                /* match length */

    test    %r8d, %r8d
    je      L(fail)
    cmp     %r12, %r8
    ja      L(fail)
    lea     (%r12,%r10,1), %r9
    cmp     %r13, %r9
    ja      L(fail)

    mov     %r12, %rsi
    sub     %r8, %rsi                       /* produced - offset */
    lea     (%rbx,%rsi,1), %rsi
    lea     (%rbx,%r12,1), %rdi
    mov     %r10, %rcx

    cmp     $OVERRUN, %r8
    jb      L(mat_near)

L(mat_wide):
    movdqu  (%rsi), %xmm0
    movdqu  16(%rsi), %xmm1
    movdqu  %xmm0, (%rdi)
    movdqu  %xmm1, 16(%rdi)
    add     $OVERRUN, %rsi
    add     $OVERRUN, %rdi
    sub     $OVERRUN, %rcx
    jg      L(mat_wide)
    jmp     L(mat_done)

L(mat_near):
    call    L(splat)

L(mat_done):
    mov     %r9, %r12
    jmp     L(block)

L(done):
    cmp     %r13, %r12
    jne     L(fail)
    mov     %r13d, %eax
    jmp     L(ret)

L(fail):
    xor     %eax, %eax

L(ret):
    pop     %r15
    pop     %r14
    pop     %r13
    pop     %r12
    pop     %rbp
    pop     %rbx
    ret
