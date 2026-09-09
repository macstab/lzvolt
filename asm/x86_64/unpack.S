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
 * for the checks below to be sufficient.
 *
 * SSE2 only, no runtime dispatch: 16-byte moves are all this needs and SSE2 is
 * the x86-64 baseline.
 *
 * # Everything is a pointer, and that is the point
 *
 * The first version of this counted `produced` as an integer and converted it
 * to an address wherever it needed one -- six base-plus-index computations in
 * the fast path, which is free on AArch64 where the addressing mode does it and
 * not free here, where an indexed store is an extra micro-op and cannot take
 * the simple address-generation path.
 *
 * So the cursor is a pointer, its bounds are pointers, and every store is
 * `(%rdi)`. The comparisons come out the same: `produced >= declared` becomes
 * `dst >= dst_end`, and `offset <= produced` becomes `dst - offset >= base`.
 * This is how LZ4 is written and the reason is the same one.
 *
 * Two more things follow from it. Both fast-path limits are loop-invariant
 * pointers now, so each is one `cmp` rather than an `lea` and a `cmp`. And the
 * offset check, which was "not zero, and not past what has been produced",
 * collapses to one unsigned comparison -- `offset - 1 < available` is false for
 * zero by wrapping, which is the same trick the packer uses on its candidates.
 *
 * # What is checked, and what is checked once
 *
 * Values arrive from sockets and snapshot files, so every length here is
 * attacker-controlled and this kernel is the boundary.
 *
 * `dst_cap >= declared + 64` on entry is what lets the loop carry no capacity
 * test: every copy is bounded against `dst_end` first, and one rounded up to
 * whole blocks then ends by `declared + 31 < dst_cap`. The fast path can write
 * further than that under the wide split, which is what OUT_MARGIN is for.
 *
 * # Measurement
 *
 * Written on Apple silicon, where x86 runs only under a binary translator, so
 * correctness is checked here and speed is not. The x86-64 CI job and
 * scripts/bench-on-gce.sh are where a number for this file comes from, and the
 * first such number said this kernel was behind liblz4 on identical blocks
 * while the AArch64 one was ahead. That is what the rewrite above is answering.
 */

#include "../common.h"

#ifdef KEVA_WIDE
#  define LIT_SHIFT  6
#  define LIT_MAX    3
#  define MAT_MAX    63
#  ifdef KEVA_AVX2
#    define KEVA_PFX   wa
#    define KEVA_ENTRY keva_unpack_wide_avx2
#  else
#    define KEVA_PFX   w
#    define KEVA_ENTRY keva_unpack_wide
#  endif
#else
#  define LIT_SHIFT  4
#  define LIT_MAX    15
#  define MAT_MAX    15
#  ifdef KEVA_AVX2
#    define KEVA_PFX   ea
#    define KEVA_ENTRY keva_unpack_avx2
#  else
#    define KEVA_PFX   e
#    define KEVA_ENTRY keva_unpack
#  endif
#endif

#define MIN_MATCH         4
#define OVERRUN           32
#define FAST_INPUT_SLACK  32
#define MAT_CAP           (MAT_MAX + MIN_MATCH)

/*
 * How much the fixed match move copies: one vector pair under the even split,
 * three under the wide one, because that is what covers a match the split can
 * carry inline. Without it the wide split would send every match past
 * twenty-eight bytes to the checked path, which is the shape it exists for.
 */
#if MAT_CAP > 64
#  define COPY_MAX   96
#elif MAT_CAP > 32
#  define COPY_MAX   64
#else
#  define COPY_MAX   32
#endif

/*
 * How far short of the end the fast loop stops: the widest thing a fast block
 * writes is its literals plus COPY_MAX and that has to land inside the caller's
 * 64 bytes of slack, plus room for the final literal run that has no offset
 * behind it and which the fast path would misread as one.
 */
#if (LIT_MAX + COPY_MAX) > 64
#  define OUT_MARGIN (LIT_MAX + COPY_MAX - 64 + 16)
#else
#  define OUT_MARGIN 16
#endif

/* Input the fast path may read past the cursor, at its widest literal run. */
#define FAST_IN (1 + (LIT_MAX - 1) + 2 + FAST_INPUT_SLACK)

#define KEVA_NAME1(n) KEVA_SYM(n)
#define KEVA_NAME(n) KEVA_NAME1(n)

#define L2(p, n) .L ## p ## _ ## n
#define L1(p, n) L2(p, n)
#define L(n) L1(KEVA_PFX, n)

/*
 * Held for the whole call, all of them pointers:
 *
 *   rbx  dst base          rdi  dst cursor      r14  src cursor
 *   rbp  fast dst limit    r12  dst end         r15  fast src limit
 *   r13  src end
 *
 * `rdi` is the cursor rather than a callee-saved register on purpose: it is
 * what `rep movsb` writes through, so the exact path's copies advance it for
 * free.
 *
 * Everything else is scratch, which matters because `rep movsb` also takes rsi
 * and rcx and returns them spent.
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

    mov     %rdi, %r14                      /* src cursor, before rdi is reused */
    lea     (%rdi,%rsi,1), %r13             /* src end */
    mov     %rdx, %rbx                      /* dst base */
    lea     (%rdx,%r8,1), %r12              /* dst end */
    lea     (%rdx,%r9,1), %rdi              /* dst cursor = base + start */

    /* The entry check the loop then never repeats. */
    lea     64(%r8), %rax
    cmp     %rax, %rcx                      /* dst_cap, still in its argument */
    jb      L(fail)

    /* Loop-invariant, so both are derived once and each costs one compare.
     *
     * The dst limit can sit below `base` for a value shorter than the margin,
     * and that needs no special case here: these are addresses, `base` is
     * nowhere near zero, and the unsigned comparison then sends every block to
     * the checked path, which is what a value that short wants anyway. */
    lea     -OUT_MARGIN(%r12), %rbp
    lea     -FAST_IN(%r13), %r15

/*
 * The loop head is aligned because the micro-op cache is fed in 32-byte
 * windows and a hot loop that straddles one is delivered from the slower
 * decoders instead. It costs a few bytes of padding once.
 */
    .p2align 5
L(block):
    cmp     %r12, %rdi
    jae     L(done)

    cmp     %r13, %r14
    jae     L(fail)
    movzbl  (%r14), %eax                    /* token */

    /* ---- fast path -------------------------------------------------- */

    cmp     %rbp, %rdi
    ja      L(slow)                         /* too near the end to read ahead */
    cmp     %r15, %r14
    ja      L(slow)                         /* too near the end of the stream */

    mov     %eax, %r10d
    shr     $LIT_SHIFT, %r10d               /* short literal */
    mov     %eax, %r11d
    and     $MAT_MAX, %r11d                 /* short match */

    /*
     * One branch for both fields rather than two.
     *
     * Either field being all ones means its length escaped the token and this
     * block belongs on the checked path. Testing that separately costs two
     * branches, and the objection to merging them was that a not-taken branch
     * is nearly free to execute -- which is true, and beside the point. The
     * micro-op cache ends a line at every branch, so a fast path this dense in
     * them is delivered from the legacy decoders at fewer micro-ops per cycle
     * than the cache would give. Branches are cheap to run and expensive to
     * hold.
     *
     * `xor` leaves zero exactly where a field escaped, and multiplying is zero
     * if either is -- four more micro-ops on the arithmetic ports, which are
     * not what this loop is short of, to take one branch out of the window.
     *
     * The `test` is not redundant: `imul` sets carry and overflow and leaves
     * the zero flag undefined, so branching straight off it is a bug the
     * differential suite catches immediately.
     */
    mov     %r10d, %ecx
    xor     $LIT_MAX, %ecx
    mov     %r11d, %edx
    xor     $MAT_MAX, %edx
    imul    %edx, %ecx
    test    %ecx, %ecx
    je      L(slow)

    movzwl  1(%r14,%r10,1), %ecx            /* offset */
    lea     (%rdi,%r10,1), %r8              /* dst after the literals */

    /*
     * One comparison for both halves of what an offset must be. `available` is
     * how far back this block may reach; `offset - 1` wraps to enormous when
     * the offset is zero, so the same unsigned test rejects it.
     */
    mov     %r8, %rax
    sub     %rbx, %rax                      /* available */
    lea     -1(%rcx), %rdx
    cmp     %rax, %rdx
    jae     L(fail)

    lea     MIN_MATCH(%r11), %r9            /* match length */
    lea     (%r8,%r9,1), %rax               /* dst after the match */
    cmp     %r12, %rax
    ja      L(fail)

    /* Fixed width on purpose -- the guards above paid for not asking again.
     * Sixteen literal bytes are stored whether or not that many were meant;
     * the surplus is overwritten by the next block or falls in the slack the
     * caller trims. */
#ifdef KEVA_AVX2
    vmovdqu 1(%r14), %xmm0
    vmovdqu %xmm0, (%rdi)
#else
    movdqu  1(%r14), %xmm0
    movdqu  %xmm0, (%rdi)
#endif

    lea     3(%r14,%r10,1), %r14            /* token + literals + offset */

    mov     %r8, %rsi
    sub     %rcx, %rsi                      /* the match source */
    cmp     $OVERRUN, %rcx
    jb      L(near)

#ifdef KEVA_AVX2
    vmovdqu (%rsi), %ymm0
    vmovdqu %ymm0, (%r8)
#  if COPY_MAX > 32
    vmovdqu 32(%rsi), %ymm1
    vmovdqu %ymm1, 32(%r8)
#  endif
#  if COPY_MAX > 64
    vmovdqu 64(%rsi), %ymm2
    vmovdqu %ymm2, 64(%r8)
#  endif
#else
    movdqu  (%rsi), %xmm0
    movdqu  16(%rsi), %xmm1
    movdqu  %xmm0, (%r8)
    movdqu  %xmm1, 16(%r8)
#  if COPY_MAX > 32
    movdqu  32(%rsi), %xmm2
    movdqu  48(%rsi), %xmm3
    movdqu  %xmm2, 32(%r8)
    movdqu  %xmm3, 48(%r8)
#  endif
#  if COPY_MAX > 64
    movdqu  64(%rsi), %xmm4
    movdqu  80(%rsi), %xmm5
    movdqu  %xmm4, 64(%r8)
    movdqu  %xmm5, 80(%r8)
#  endif
#endif
    mov     %rax, %rdi
    jmp     L(block)

L(near):
    mov     %rax, %r10                      /* keep the new cursor */
    mov     %r8, %rdi
    mov     %r9, %rcx
    call    L(splat)
    mov     %r10, %rdi
    jmp     L(block)

    /* ---- the pattern splat ------------------------------------------ */

/*
 * Fill `rcx` bytes at `rdi` from `rsi`, the two closer together than one block.
 *
 * A wide load would read bytes that have not been written, so the run is grown
 * instead: what is already there is copied onto itself, doubling the distance
 * each time, until the gap is wide enough for vector moves. From an offset of
 * one that is 1 + 2 + 4 + 8 byte moves before the loop takes over, against one
 * byte move per output byte, which is what this replaced.
 *
 * Reached by `call` from two sites. Nothing else here touches the stack, so the
 * return address is all that is on it.
 */
L(splat):
    mov     %rdi, %rdx
    sub     %rsi, %rdx                      /* gap */

L(splat_grow):
    cmp     $16, %rdx
    jae     L(splat_wide)
    mov     %rdx, %r11                      /* append `gap` bytes */
    mov     %rdi, %rsi
    sub     %rdx, %rsi
L(splat_byte):
    movzbl  (%rsi), %eax
    mov     %al, (%rdi)
    inc     %rsi
    inc     %rdi
    dec     %r11
    jnz     L(splat_byte)
    sub     %rdx, %rcx
    jle     L(splat_ret)
    add     %rdx, %rdx                      /* the gap has doubled */
    jmp     L(splat_grow)

L(splat_wide):
    mov     %rdi, %rsi
    sub     %rdx, %rsi
L(splat_loop):
#ifdef KEVA_AVX2
    vmovdqu (%rsi), %xmm0
    vmovdqu %xmm0, (%rdi)
#else
    movdqu  (%rsi), %xmm0
    movdqu  %xmm0, (%rdi)
#endif
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
    cmp     %r13, %r14
    jae     L(fail)
    movzbl  (%r14), %eax
    inc     %r14
    add     %rax, %r10
    /* Bounded against what is left to produce rather than against overflow:
     * nothing beyond it can be written, so a chain that reaches it has already
     * failed. */
    mov     %r12, %rdx
    sub     %rbx, %rdx
    cmp     %rdx, %r10
    ja      L(fail)
    cmp     $255, %eax
    je      L(lit_ext)
    add     $LIT_MAX, %r10

L(lit_len):
    lea     (%r14,%r10,1), %rax             /* end of the run in the input */
    cmp     %r13, %rax
    ja      L(fail)
    lea     (%rdi,%r10,1), %r9              /* dst after the literals */
    cmp     %r12, %r9
    ja      L(fail)

    test    %r10, %r10
    je      L(lit_done)

    mov     %r14, %rsi

    /* Blocks only while a whole one is readable; past that the exact bytes,
     * because overrunning the input would read memory that is not ours. */
    lea     OVERRUN(%rax), %rdx
    cmp     %r13, %rdx
    ja      L(lit_bytes)

    mov     %r10, %rcx
L(lit_wide):
#ifdef KEVA_AVX2
    vmovdqu (%rsi), %ymm0
    vmovdqu %ymm0, (%rdi)
#else
    movdqu  (%rsi), %xmm0
    movdqu  16(%rsi), %xmm1
    movdqu  %xmm0, (%rdi)
    movdqu  %xmm1, 16(%rdi)
#endif
    add     $OVERRUN, %rsi
    add     $OVERRUN, %rdi
    sub     $OVERRUN, %rcx
    /* Signed: a length below one block lands at or under zero and stops. An
     * unsigned test wraps and never terminates. */
    jg      L(lit_wide)
    jmp     L(lit_done)

L(lit_bytes):
    mov     %r10, %rcx
    rep movsb

L(lit_done):
    lea     (%r14,%r10,1), %r14             /* at = end */
    mov     %r9, %rdi                       /* and the cursor, exactly */

    cmp     %r12, %rdi
    je      L(done)

    lea     2(%r14), %rax
    cmp     %r13, %rax
    ja      L(fail)
    movzwl  (%r14), %ecx                    /* offset */
    mov     %rax, %r14

    mov     %r11d, %r10d
    and     $MAT_MAX, %r10d
    cmp     $MAT_MAX, %r10d
    jne     L(mat_len)

    xor     %r10, %r10
L(mat_ext):
    cmp     %r13, %r14
    jae     L(fail)
    movzbl  (%r14), %eax
    inc     %r14
    add     %rax, %r10
    mov     %r12, %rdx
    sub     %rbx, %rdx
    cmp     %rdx, %r10
    ja      L(fail)
    cmp     $255, %eax
    je      L(mat_ext)
    add     $MAT_MAX, %r10

L(mat_len):
    add     $MIN_MATCH, %r10                /* match length */

    /* The same single comparison as the fast path. */
    mov     %rdi, %rax
    sub     %rbx, %rax                      /* available */
    lea     -1(%rcx), %rdx
    cmp     %rax, %rdx
    jae     L(fail)

    lea     (%rdi,%r10,1), %r9              /* dst after the match */
    cmp     %r12, %r9
    ja      L(fail)

    mov     %rdi, %rsi
    sub     %rcx, %rsi                      /* the match source */

    cmp     $OVERRUN, %rcx
    jb      L(mat_near)

    mov     %r10, %rcx
L(mat_wide):
#ifdef KEVA_AVX2
    vmovdqu (%rsi), %ymm0
    vmovdqu %ymm0, (%rdi)
#else
    movdqu  (%rsi), %xmm0
    movdqu  16(%rsi), %xmm1
    movdqu  %xmm0, (%rdi)
    movdqu  %xmm1, 16(%rdi)
#endif
    add     $OVERRUN, %rsi
    add     $OVERRUN, %rdi
    sub     $OVERRUN, %rcx
    jg      L(mat_wide)
    jmp     L(mat_done)

L(mat_near):
    mov     %r10, %rcx
    call    L(splat)

L(mat_done):
    mov     %r9, %rdi                       /* the cursor, exactly */
    jmp     L(block)

L(done):
    cmp     %r12, %rdi
    jne     L(fail)
    mov     %r12, %rax
    sub     %rbx, %rax
    jmp     L(ret)

L(fail):
    xor     %eax, %eax

L(ret):
#ifdef KEVA_AVX2
    vzeroupper
#endif
    pop     %r15
    pop     %r14
    pop     %r13
    pop     %r12
    pop     %rbp
    pop     %rbx
    ret
