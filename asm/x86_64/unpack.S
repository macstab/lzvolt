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
 * SSE2 is the baseline and 16-byte moves are all the copying needs. One thing
 * beyond it is asked for at run time: `pshufb`, which builds a near match's
 * repeating pattern in a single shuffle. Everything else is identical between
 * the two builds, and the SSE2 one is what a machine without SSSE3 gets.
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

/*
 * One kernel per part line.
 *
 * The same source builds three pairs: a Xeon pair, an EPYC pair, and an SSE2
 * pair for anything else. They started identical and are expected not to stay
 * that way -- the last literal run already wants `rep movsb` on one and a copy
 * ladder on the other, measured at +17% and -9% for the same change, and a
 * compromise between those is worse than either. The vendor is asked once, at
 * the first call.
 *
 * Both vendor builds may assume SSSE3: every part either house has shipped
 * since 2007 has it, and the SSE2 pair is what a machine without it gets.
 */
#if defined(KEVA_XEON) || defined(KEVA_EPYC)
#  define KEVA_SSSE3 1
#endif

#ifdef KEVA_WIDE
#  if defined(KEVA_XEON)
#    define KEVA_PFX   wx
#    define KEVA_ENTRY keva_unpack_wide_xeon
#  elif defined(KEVA_EPYC)
#    define KEVA_PFX   wp
#    define KEVA_ENTRY keva_unpack_wide_epyc
#  elif defined(KEVA_SSSE3)
#    define KEVA_PFX   ws
#    define KEVA_ENTRY keva_unpack_wide_ssse3
#  else
#    define KEVA_PFX   w
#    define KEVA_ENTRY keva_unpack_wide
#  endif
#  define LIT_SHIFT  6
#  define LIT_MAX    3
#  define MAT_MAX    63
#else
#  if defined(KEVA_XEON)
#    define KEVA_PFX   ex
#    define KEVA_ENTRY keva_unpack_xeon
#  elif defined(KEVA_EPYC)
#    define KEVA_PFX   ep
#    define KEVA_ENTRY keva_unpack_epyc
#  elif defined(KEVA_SSSE3)
#    define KEVA_PFX   es
#    define KEVA_ENTRY keva_unpack_ssse3
#  else
#    define KEVA_PFX   e
#    define KEVA_ENTRY keva_unpack
#  endif
#  define LIT_SHIFT  4
#  define LIT_MAX    15
#  define MAT_MAX    15
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

#define L1(p, n) KEVA_LABEL(p, n)
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
    /*
     * Two branches, not four, and the reason is an ordering rather than a
     * guess.
     *
     * The head used to ask four questions per block: is the output finished, is
     * the input exhausted, is the output too near its end to read ahead, is the
     * input. But `rbp` is `r12 - OUT_MARGIN` and `r15` is `r13 - FAST_IN`, so
     * each pair is a wide limit and a narrow one on the same cursor, and the
     * narrow one implies the wide. A cursor inside the zone cannot be past the
     * end. The two wide tests were unreachable while the loop was running and
     * only decided which way to leave once it stopped.
     *
     * So the zone is tested here and the reason is worked out on the cold side,
     * where it costs nothing. Leaving is once per value; staying is every block.
     *
     * The input test goes first so the token load has one branch in front of it
     * instead of two. That load starts the block's dependency chain -- token,
     * literal count, offset address, offset -- and every cycle it waits is a
     * cycle the whole block waits.
     */
    cmp     %r15, %r14
    ja      L(src_edge)                     /* too near the end of the stream */
    movzbl  (%r14), %eax                    /* token */

    /* ---- fast path -------------------------------------------------- */

    cmp     %rbp, %rdi
    ja      L(dst_edge)                     /* too near the end to read ahead */

    mov     %eax, %r10d
    shr     $LIT_SHIFT, %r10d               /* short literal */
    mov     %eax, %r11d
    and     $MAT_MAX, %r11d                 /* short match */
    lea     1(%r14), %rsi                   /* where the literals begin */
    cmp     $LIT_MAX, %r10d
    je      L(fast_litext)

    movzwl  (%rsi,%r10,1), %ecx             /* offset */
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

    /*
     * A match whose length escaped the token used to leave for the checked
     * path, and counting says that is most of them: at 512 bytes of records
     * only a third of blocks reached the fast path at all, and six of the eight
     * that did not were here. Matches in a store's values are long -- that is
     * what the wide split exists for -- so "long match" is the common case and
     * sending it away was sending away the work.
     *
     * It is read here instead. The chain is bounded against the end of the
     * stream and against what is left to produce, exactly as the checked path
     * bounds it, because those bytes are as attacker-controlled as any other.
     */
    /* Fixed width on purpose -- the guards above paid for not asking again.
     * Sixteen literal bytes are stored whether or not that many were meant;
     * the surplus is overwritten by the next block or falls in the slack the
     * caller trims. */
    movdqu  (%rsi), %xmm0
    movdqu  %xmm0, (%rdi)

L(fast_litdone):
    lea     2(%rsi,%r10,1), %r14            /* past the literals and the offset */

    /*
     * A match whose length escaped the token used to leave for the checked
     * path, and counting says that is most of them: at 512 bytes of records
     * only a third of blocks reached the fast path, and six of the eight that
     * did not were here. Matches in a store's values are long -- that is what
     * the wide split exists for -- so sending "long match" away was sending
     * away the work.
     *
     * It is read here instead, after the cursor has passed the offset, which
     * is where the chain begins. Bounded against the end of the stream and
     * against what is left to produce, exactly as the checked path bounds it.
     */
    lea     MIN_MATCH(%r11), %r9            /* match length, so far */
    cmp     $MAT_MAX, %r11d
    je      L(fast_matext)

L(fast_matgo):
    lea     (%r8,%r9,1), %rax               /* dst after the match */
    cmp     %r12, %rax
    ja      L(fail)

    mov     %r8, %rsi
    sub     %rcx, %rsi                      /* the match source */
    cmp     $OVERRUN, %rcx
    jb      L(near)
    cmp     $COPY_MAX, %r9
    ja      L(fast_long)

    movdqu  (%rsi), %xmm0
    movdqu  16(%rsi), %xmm1
    movdqu  %xmm0, (%r8)
    movdqu  %xmm1, 16(%r8)
#if COPY_MAX > 32
    movdqu  32(%rsi), %xmm2
    movdqu  48(%rsi), %xmm3
    movdqu  %xmm2, 32(%r8)
    movdqu  %xmm3, 48(%r8)
#endif
#if COPY_MAX > 64
    movdqu  64(%rsi), %xmm4
    movdqu  80(%rsi), %xmm5
    movdqu  %xmm4, 64(%r8)
    movdqu  %xmm5, 80(%r8)
#endif
    mov     %rax, %rdi
    jmp     L(block)

L(fast_litext):
    /*
     * A literal run whose length escaped the token.
     *
     * With long matches handled above, this is the only thing still sending
     * blocks to the checked path: 738 of the 1340 misses on 64 KiB of varied,
     * and the last quarter of blocks everywhere else. Counting the share
     * before and after the match change -- 17% to 42% at 512 bytes of records,
     * 56% to 76% on varied -- is what says this is the remaining one.
     *
     * It rejoins at `L(fast_litdone)` with `rsi` and `r10` describing the run
     * it has already copied, which is exactly what the short case leaves
     * behind, so everything from the match onward is shared.
     */
    xor     %r10, %r10
L(fast_litchain):
    cmp     %r13, %rsi
    jae     L(fail)
    movzbl  (%rsi), %edx
    inc     %rsi
    add     %rdx, %r10
    /* Against what is left to produce; nothing beyond it can be written. */
    mov     %r12, %rax
    sub     %rbx, %rax
    cmp     %rax, %r10
    ja      L(fail)
    cmp     $255, %edx
    je      L(fast_litchain)
    add     $LIT_MAX, %r10

    /* Past the token's own width the entry guard covers nothing, so the run
     * and the offset behind it are bounded here, as the checked path does. */
    lea     2(%rsi,%r10,1), %rax
    cmp     %r13, %rax
    ja      L(fail)
    lea     (%rdi,%r10,1), %r8              /* dst after the literals */
    cmp     %r12, %r8
    ja      L(fail)

    movzwl  (%rsi,%r10,1), %ecx             /* offset */
    test    %ecx, %ecx
    je      L(fail)
    mov     %r8, %rax
    sub     %rbx, %rax
    cmp     %rax, %rcx
    ja      L(fail)

    /* Copied through scratch, because `rsi` and `rcx` are wanted after it. */
    mov     %rsi, %r9                       /* source walker */
    mov     %rdi, %rdx                      /* destination walker */
    lea     OVERRUN(%rsi,%r10,1), %rax
    cmp     %r13, %rax
    ja      L(fast_litslow)
    mov     %r10, %rax
L(fast_litblock):
    movdqu  (%r9), %xmm0
    movdqu  16(%r9), %xmm1
    movdqu  %xmm0, (%rdx)
    movdqu  %xmm1, 16(%rdx)
    add     $OVERRUN, %r9
    add     $OVERRUN, %rdx
    sub     $OVERRUN, %rax
    jg      L(fast_litblock)
    jmp     L(fast_litdone)

L(fast_litslow):
    /* The tail of a stream: one byte at a time, because a block would read
     * past what is ours. */
    mov     %r10, %rax
L(fast_litbyte):
    /* Through r14, which is the only scratch left here and is recomputed at
     * the rejoin anyway. Not r11: that is holding the token's match field, and
     * the token is long behind us -- the first version of this tried to read
     * it back from the last chain byte. */
    movzbl  (%r9), %r14d
    mov     %r14b, (%rdx)
    inc     %r9
    inc     %rdx
    dec     %rax
    jnz     L(fast_litbyte)
    jmp     L(fast_litdone)

/*
 * The first extension byte, which is nearly always the only one.
 *
 * A match length escapes as soon as it passes what the token carries -- 18
 * bytes under the even split -- and on an LZ4 block, whose token is always 4/4,
 * that is most matches. This is the hot path of the shape that is furthest
 * behind, and it was written as a loop paying loop prices on a body that runs
 * once: a test against the end of the output per byte, and a jump back.
 *
 * The output test is not needed for the first byte, and that is provable rather
 * than likely. After one byte the length is at most MIN_MATCH + MAT_MAX + 254 --
 * 273 under either split -- and `L(fast_matgo)` already forms `dst + length` and
 * rejects it against `dst_end`. An addition that small cannot carry past the end
 * of the address space, so the test here is the same test done twice.
 *
 * It stays in the loop below, because a chain of 255s has no bound of its own
 * and that is where a length could grow until it wraps.
 *
 * So the common case reads, adds, and falls through on a not-taken compare: six
 * instructions and two branches where there were eleven and three.
 */
L(fast_matext):
    cmp     %r13, %r14
    jae     L(fail)
    movzbl  (%r14), %edx
    inc     %r14
    add     %rdx, %r9
    cmp     $255, %edx
    jne     L(fast_matgo)

L(fast_matchain):
    cmp     %r13, %r14
    jae     L(fail)
    movzbl  (%r14), %edx
    inc     %r14
    add     %rdx, %r9
    /* Bounded against what is left to produce: nothing beyond it can be
     * written, so a chain that reaches it has already failed.
     *
     * Through `rax`, not `rcx`: `rcx` is holding the offset, which was read
     * before the chain and is wanted after it. Clobbering it here is what the
     * first version of this did, and the differential suite said so. */
    mov     %r12, %rax
    sub     %rbx, %rax
    cmp     %rax, %r9
    ja      L(fail)
    cmp     $255, %edx
    je      L(fast_matchain)
    jmp     L(fast_matgo)

L(fast_long):
    /* Longer than the fixed move: the same block loop the checked path uses,
     * and the same argument for overrunning -- `dst_cap` is `declared + 64`
     * and this ends by `declared + 31`. */
    mov     %rax, %r10                      /* keep the new cursor */
    mov     %r8, %rdi
    mov     %r9, %rcx
L(fast_longloop):
    movdqu  (%rsi), %xmm0
    movdqu  16(%rsi), %xmm1
    movdqu  %xmm0, (%rdi)
    movdqu  %xmm1, 16(%rdi)
    add     $OVERRUN, %rsi
    add     $OVERRUN, %rdi
    sub     $OVERRUN, %rcx
    jg      L(fast_longloop)
    mov     %r10, %rdi
    jmp     L(block)

/*
 * A near match, written where it is found.
 *
 * This used to be a `call`, which made sense while the routine behind it was a
 * byte-at-a-time doubling loop. With the shuffle it is eight instructions, and
 * a call frame for eight instructions is the wrong way round: the `call` and
 * the `ret` are two micro-ops, the return-address predictor takes a slot, and
 * four `mov` exist for no reason but to name arguments.
 *
 * Inlined it needs none of them, and it gets something the routine could not
 * have: the gap is already in `rcx` as the offset, so the subtraction that
 * derived it disappears too.
 *
 * Live here: rsi the match source, r8 where it is written, r9 its length, rcx
 * the offset -- which is below OVERRUN, or this label was not reached -- and
 * rax the cursor the loop resumes with. rdx is dead since the offset check.
 *
 * The cold call site keeps the routine. It runs once per value at most.
 */
L(near):
#ifdef KEVA_SSSE3
    cmp     $16, %rcx
    jae     L(near_wide)

    movdqu  (%rsi), %xmm0                   /* the live bytes, and some noise */
    shl     $4, %ecx                        /* offset -> mask index */
    lea     L(splat_first)(%rip), %rdx
    pshufb  (%rdx,%rcx,1), %xmm0
    lea     L(splat_next)(%rip), %rdx
    movdqa  (%rdx,%rcx,1), %xmm1
    mov     %r8, %rdx
L(near_pattern):
    movdqu  %xmm0, (%rdx)
    add     $16, %rdx
    pshufb  %xmm1, %xmm0
    sub     $16, %r9
    jg      L(near_pattern)
    mov     %rax, %rdi
    jmp     L(block)

/* Sixteen to thirty-one apart: far enough for whole vectors, near enough that
 * the fixed move above would have read what it had not written. */
L(near_wide):
    mov     %r8, %rdx
L(near_wideloop):
    movdqu  (%rsi), %xmm0
    movdqu  %xmm0, (%rdx)
    add     $16, %rsi
    add     $16, %rdx
    sub     $16, %r9
    jg      L(near_wideloop)
    mov     %rax, %rdi
    jmp     L(block)
#else
    mov     %rax, %r10                      /* keep the new cursor */
    mov     %r8, %rdi
    mov     %r9, %rcx
    call    L(splat)
    mov     %r10, %rdi
    jmp     L(block)
#endif

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

#ifdef KEVA_SSSE3
    /*
     * The whole pattern in one shuffle, and then one shuffle per store.
     *
     * The output of a repeat is periodic: out[i] = S[i mod d], for the d bytes
     * S that sit behind the cursor. A 16-byte window of that is a shuffle of
     * those d bytes, so the first window is `pshufb` with the mask j -> j mod d.
     *
     * The window after it is a shuffle of the window before it, and by the same
     * mask every time: window k holds S[(16k + j) mod d], so window k+1 wants
     * S[(16k + 16 + j) mod d], which is element (j + 16) mod d of window k.
     * That index is below d and therefore below sixteen, so it is a legal
     * shuffle index and the mask does not depend on k.
     *
     * Two tables of sixteen masks, and the loop becomes store, shuffle, count.
     * What it replaces is a byte at a time until the gap reaches sixteen: from
     * an offset of one that was fifteen single-byte moves before any vector
     * work, and offsets that small are what `varied` data is full of.
     *
     * The load reads sixteen bytes where only d are meant. The rest is whatever
     * the destination holds and the shuffle discards it; the read cannot leave
     * the caller's buffer, which is `declared + 64` and holds this cursor.
     */
    movdqu  (%rsi), %xmm0                   /* d live bytes at the front */
    shl     $4, %edx                        /* mask index: d * 16 */
    lea     L(splat_first)(%rip), %rax
    pshufb  (%rax,%rdx,1), %xmm0
    lea     L(splat_next)(%rip), %rax
    movdqa  (%rax,%rdx,1), %xmm1
L(splat_pattern):
    movdqu  %xmm0, (%rdi)
    add     $16, %rdi
    pshufb  %xmm1, %xmm0                    /* advance the window by sixteen */
    sub     $16, %rcx
    jg      L(splat_pattern)
    ret
#endif

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
    movdqu  (%rsi), %xmm0
    movdqu  %xmm0, (%rdi)
    add     $16, %rsi
    add     $16, %rdi
    sub     $16, %rcx
    jg      L(splat_loop)

L(splat_ret):
    ret

#ifdef KEVA_SSSE3
/*
 * The shuffle masks, indexed by the offset in bytes.
 *
 * `splat_first` turns d live bytes into a full sixteen-byte window: j -> j mod
 * d. `splat_next` advances a window by sixteen: j -> (j + 16) mod d, which is
 * the same mask for every window and is why the loop above carries no state
 * beyond the vector itself.
 *
 * Sixteen-byte aligned because a legacy SSE memory operand must be, and the
 * index is d * 16 so every entry lands on a boundary. Row zero is never read;
 * an offset of zero is rejected long before this.
 *
 * They sit in .text rather than a read-only section so that one directive does
 * not have to be spelled two ways for ELF and Mach-O. Five hundred and twelve
 * bytes, touched only by a near match.
 */
    .p2align 4
L(splat_first):
    .byte    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0    /* d =  0 */
    .byte    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0    /* d =  1 */
    .byte    0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1    /* d =  2 */
    .byte    0, 1, 2, 0, 1, 2, 0, 1, 2, 0, 1, 2, 0, 1, 2, 0    /* d =  3 */
    .byte    0, 1, 2, 3, 0, 1, 2, 3, 0, 1, 2, 3, 0, 1, 2, 3    /* d =  4 */
    .byte    0, 1, 2, 3, 4, 0, 1, 2, 3, 4, 0, 1, 2, 3, 4, 0    /* d =  5 */
    .byte    0, 1, 2, 3, 4, 5, 0, 1, 2, 3, 4, 5, 0, 1, 2, 3    /* d =  6 */
    .byte    0, 1, 2, 3, 4, 5, 6, 0, 1, 2, 3, 4, 5, 6, 0, 1    /* d =  7 */
    .byte    0, 1, 2, 3, 4, 5, 6, 7, 0, 1, 2, 3, 4, 5, 6, 7    /* d =  8 */
    .byte    0, 1, 2, 3, 4, 5, 6, 7, 8, 0, 1, 2, 3, 4, 5, 6    /* d =  9 */
    .byte    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 0, 1, 2, 3, 4, 5    /* d = 10 */
    .byte    0, 1, 2, 3, 4, 5, 6, 7, 8, 9,10, 0, 1, 2, 3, 4    /* d = 11 */
    .byte    0, 1, 2, 3, 4, 5, 6, 7, 8, 9,10,11, 0, 1, 2, 3    /* d = 12 */
    .byte    0, 1, 2, 3, 4, 5, 6, 7, 8, 9,10,11,12, 0, 1, 2    /* d = 13 */
    .byte    0, 1, 2, 3, 4, 5, 6, 7, 8, 9,10,11,12,13, 0, 1    /* d = 14 */
    .byte    0, 1, 2, 3, 4, 5, 6, 7, 8, 9,10,11,12,13,14, 0    /* d = 15 */

L(splat_next):
    .byte    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0    /* d =  0 */
    .byte    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0    /* d =  1 */
    .byte    0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1    /* d =  2 */
    .byte    1, 2, 0, 1, 2, 0, 1, 2, 0, 1, 2, 0, 1, 2, 0, 1    /* d =  3 */
    .byte    0, 1, 2, 3, 0, 1, 2, 3, 0, 1, 2, 3, 0, 1, 2, 3    /* d =  4 */
    .byte    1, 2, 3, 4, 0, 1, 2, 3, 4, 0, 1, 2, 3, 4, 0, 1    /* d =  5 */
    .byte    4, 5, 0, 1, 2, 3, 4, 5, 0, 1, 2, 3, 4, 5, 0, 1    /* d =  6 */
    .byte    2, 3, 4, 5, 6, 0, 1, 2, 3, 4, 5, 6, 0, 1, 2, 3    /* d =  7 */
    .byte    0, 1, 2, 3, 4, 5, 6, 7, 0, 1, 2, 3, 4, 5, 6, 7    /* d =  8 */
    .byte    7, 8, 0, 1, 2, 3, 4, 5, 6, 7, 8, 0, 1, 2, 3, 4    /* d =  9 */
    .byte    6, 7, 8, 9, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 0, 1    /* d = 10 */
    .byte    5, 6, 7, 8, 9,10, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9    /* d = 11 */
    .byte    4, 5, 6, 7, 8, 9,10,11, 0, 1, 2, 3, 4, 5, 6, 7    /* d = 12 */
    .byte    3, 4, 5, 6, 7, 8, 9,10,11,12, 0, 1, 2, 3, 4, 5    /* d = 13 */
    .byte    2, 3, 4, 5, 6, 7, 8, 9,10,11,12,13, 0, 1, 2, 3    /* d = 14 */
    .byte    1, 2, 3, 4, 5, 6, 7, 8, 9,10,11,12,13,14, 0, 1    /* d = 15 */
#endif

/*
 * Why the loop stopped, worked out where it is cold.
 *
 * Both arrive knowing one cursor left the zone. Neither knows why, and the
 * order they ask in is the order the old head asked in, so the answers are the
 * same ones: a finished output outranks an exhausted input, because a value
 * whose last block runs to the final byte leaves the source cursor at the end
 * and that is success, not truncation.
 */
L(src_edge):
    cmp     %r12, %rdi
    jae     L(done)
    cmp     %r13, %r14
    jae     L(fail)
    /* The checked path is entered with the token already in `eax`, which the
     * loop head used to load for everyone. It cannot load it any more -- that
     * is the branch this removed -- so the one entry that skipped the load does
     * it here, where the compare above has just made it safe. */
    movzbl  (%r14), %eax
    jmp     L(slow)

L(dst_edge):
    cmp     %r12, %rdi
    jae     L(done)
    jmp     L(slow)

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
    movdqu  (%rsi), %xmm0
    movdqu  16(%rsi), %xmm1
    movdqu  %xmm0, (%rdi)
    movdqu  %xmm1, 16(%rdi)
    add     $OVERRUN, %rsi
    add     $OVERRUN, %rdi
    sub     $OVERRUN, %rcx
    /* Signed: a length below one block lands at or under zero and stops. An
     * unsigned test wraps and never terminates. */
    jg      L(lit_wide)
    jmp     L(lit_done)

/*
 * The last literal run of a value, in blocks and then two overlapping moves.
 *
 * This was `rep movsb`. On x86 that is a string instruction with a start-up
 * cost measured in tens of cycles before it moves a byte -- fast once it is
 * running, and this is never running long. It is reached when the run ends
 * within one block of the end of the input, which is to say once per value,
 * and the run is usually a handful of bytes. liblz4 does not use it anywhere;
 * it copies in eight-byte blocks and finishes with overlapping moves, and the
 * AArch64 kernel here has had the same ladder from the start.
 *
 * Sixteen at a time while sixteen remain, because a run of `rcx` bytes ends at
 * or before the end of the input, so reading sixteen while `rcx >= 16` cannot
 * leave it. Then one pair of overlapping moves for the rest: a run of six is
 * four bytes from the front and four from the back, the middle two written
 * twice, exact and branch-free below the width test.
 */
L(lit_bytes):
    mov     %r10, %rcx
#ifdef KEVA_XEON
    /*
     * Ice Lake and later have Fast Short REP MOV, and there the string
     * instruction is the right answer for a run of a dozen bytes: +8.9% on a
     * Xeon 8481C against the ladder below.
     */
    rep movsb
    jmp     L(lit_done)
#endif

L(lit_b16):
    cmp     $16, %rcx
    jb      L(lit_tail)
    movdqu  (%rsi), %xmm0
    movdqu  %xmm0, (%rdi)
    add     $16, %rsi
    add     $16, %rdi
    sub     $16, %rcx
    jmp     L(lit_b16)

L(lit_tail):
    test    %rcx, %rcx
    je      L(lit_done)
    cmp     $8, %rcx
    jb      L(lit_t4)
    lea     -8(%rcx), %rdx
    mov     (%rsi), %rax
    mov     %rax, (%rdi)
    mov     (%rsi,%rdx,1), %rax
    mov     %rax, (%rdi,%rdx,1)
    jmp     L(lit_done)
L(lit_t4):
    cmp     $4, %rcx
    jb      L(lit_t2)
    lea     -4(%rcx), %rdx
    mov     (%rsi), %eax
    mov     %eax, (%rdi)
    mov     (%rsi,%rdx,1), %eax
    mov     %eax, (%rdi,%rdx,1)
    jmp     L(lit_done)
L(lit_t2):
    cmp     $2, %rcx
    jb      L(lit_t1)
    lea     -2(%rcx), %rdx
    movzwl  (%rsi), %eax
    mov     %ax, (%rdi)
    movzwl  (%rsi,%rdx,1), %eax
    mov     %ax, (%rdi,%rdx,1)
    jmp     L(lit_done)
L(lit_t1):
    movzbl  (%rsi), %eax
    mov     %al, (%rdi)

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
    pop     %r15
    pop     %r14
    pop     %r13
    pop     %r12
    pop     %rbp
    pop     %rbx
    ret
