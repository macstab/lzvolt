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
 * SSE2 only, no runtime dispatch: a 16-byte store is all this needs and SSE2 is
 * the x86-64 baseline, so nothing tests for a feature in front of the call.
 *
 * # What is checked, and what is checked once
 *
 * Values arrive from sockets and snapshot files, so every length here is
 * attacker-controlled and this kernel is the boundary.
 *
 * Requiring `dst_cap >= declared + 64` on entry makes the per-block output
 * check answer itself: `produced < declared` holds at the top of every block,
 * so `produced + 64 < declared + 64 <= dst_cap`. Every store below is inside
 * that window and the loop carries no capacity test. The input side is checked
 * per block, because `src_len` bounds nothing about how far a chain of length
 * bytes can run.
 *
 * # Why the copies are shaped the way they are
 *
 * The fast path stores fixed widths -- sixteen literal bytes and one 32-byte
 * block -- because its guard has already bought the right not to ask about
 * lengths again. That is where LZ4 gets most of its decode speed and it is the
 * whole reason this file exists.
 *
 * Off the fast path, both copies run in 32-byte blocks and overrun freely --
 * the same shape the portable decoder and the AArch64 kernel use, and for the
 * same reason. `rep movsb` was the first version of this and it is the wrong
 * instruction here: its startup is twenty to forty cycles on most parts, and
 * the exact path is entered precisely when a literal run reached LIT_MAX, so
 * the runs it copies are tens of bytes -- long enough to matter, far too short
 * to amortise a string operation.
 *
 * It stays for the two cases where blocks are unsound. A match closer than one
 * block to what it copies would read bytes it has not written, and byte at a
 * time forward is exactly the pattern replication the encoder meant. And the
 * tail of a stream, where overrunning the input would read memory that is not
 * ours.
 *
 * Overrunning the *output* needs no test at all: `dst_cap >= declared + 64` on
 * entry and `produced + len <= declared` before the copy give
 * `produced + round_up(len, 32) <= declared + 31 < dst_cap`. The guard is the
 * entry check, already paid for.
 *
 * # Measurement
 *
 * Written on Apple silicon, where x86 runs only under a binary translator.
 * Correctness is verified there against the portable decoder -- byte for byte,
 * including every truncation and every single-bit flip -- but the timing of a
 * translated instruction stream means nothing, so none of the choices here are
 * tuned. They mirror the AArch64 kernel and the portable decoder, both of which
 * were tuned by measurement.
 *
 * The x86-64 CI job runs scripts/packer-report.sh. That is where a number for
 * this file comes from. Until then the only claim made for it is correctness.
 *
 *   OVERRUN 32   FAST_LITERAL 16   FAST_INPUT_SLACK 32   MIN_MATCH 4
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

#define OVERRUN           32
#define FAST_INPUT_SLACK  32
#define MIN_MATCH         4

/*
 * Held for the whole call:
 *
 *   rbx  dst base        r13  declared      r15  src end
 *   r12  produced        r14  src cursor     r11  token, on the exact path
 *
 * Everything else is scratch, which matters because `rep movsb` takes rsi, rdi
 * and rcx and returns them spent -- so nothing that has to survive a copy may
 * live there. `dst_cap` is read once on entry and never again; the check it
 * feeds is what makes that possible.
 */

    .text
    .p2align 4
    .globl KEVA_NAME(KEVA_ENTRY)

KEVA_NAME(KEVA_ENTRY):
    push    %rbx
    push    %r12
    push    %r13
    push    %r14
    push    %r15

    mov     %rdi, %r14                      /* src cursor */
    lea     (%rdi,%rsi,1), %r15             /* src end */
    mov     %rdx, %rbx                      /* dst base */
    mov     %r8,  %r13                      /* declared */
    mov     %r9,  %r12                      /* produced = start */

    /* The one output check; see the note above. */
    lea     64(%r13), %rax
    cmp     %rax, %rcx                      /* dst_cap, still in its argument */
    jb      L(fail)

L(block):
    cmp     %r13, %r12
    jae     L(done)

    cmp     %r15, %r14
    jae     L(fail)
    movzbl  (%r14), %eax                    /* token */

    /* ---- fast path -------------------------------------------------- */

    mov     %eax, %r10d
    shr     $LIT_SHIFT, %r10d               /* short literal */
    cmp     $LIT_MAX, %r10d
    je      L(slow)

    mov     %eax, %r11d
    and     $MAT_MAX, %r11d                 /* short match */
    cmp     $MAT_MAX, %r11d
    je      L(slow)

    /* One fixed block is all the match copy below moves. */
    cmp     $(OVERRUN - MIN_MATCH), %r11d
    ja      L(slow)

    /* token + literals + offset + one readable block */
    lea     (1 + 2 + FAST_INPUT_SLACK)(%r14,%r10,1), %rcx
    cmp     %r15, %rcx
    ja      L(slow)

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

    /* Fixed width on purpose -- the guard above paid for not asking again.
     * Sixteen literal bytes are stored whether or not that many were meant;
     * the surplus is overwritten by the next block or falls in the slack the
     * caller trims. */
    movdqu  1(%r14), %xmm0
    movdqu  %xmm0, (%rbx,%r12,1)

    /* at = token + literals + offset, before rcx is spent below. */
    lea     3(%r14,%r10,1), %r14

    mov     %r8, %rdi
    sub     %rcx, %rdi                      /* after_literals - offset */
    cmp     $OVERRUN, %rcx
    jb      L(fast_near)

    movdqu  (%rbx,%rdi,1), %xmm0
    movdqu  16(%rbx,%rdi,1), %xmm1
    movdqu  %xmm0, (%rbx,%r8,1)
    movdqu  %xmm1, 16(%rbx,%r8,1)
    jmp     L(fast_end)

L(fast_near):
    /* Closer than a block, so a wide load would read bytes not yet written.
     * Forward, byte at a time, is the pattern replication the encoder meant. */
    lea     (%rbx,%rdi,1), %rsi
    lea     (%rbx,%r8,1), %rdi
    mov     %r9, %rcx
    rep movsb

L(fast_end):
    lea     (%r8,%r9,1), %r12               /* produced = after + match */
    jmp     L(block)

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
    /* The run must be readable, and must fit what is left to produce. */
    lea     (%r14,%r10,1), %rax
    cmp     %r15, %rax
    ja      L(fail)
    lea     (%r12,%r10,1), %r9              /* produced after the literals */
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
     * unsigned test here would wrap and never terminate, which is what the
     * first version of this loop did. */
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

    /* offset */
    lea     2(%r14), %rax
    cmp     %r15, %rax
    ja      L(fail)
    movzwl  (%r14), %r8d
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
    jb      L(mat_bytes)

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

L(mat_bytes):
    rep movsb

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
    pop     %rbx
    ret
