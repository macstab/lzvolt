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

/*
 * And one body per format.
 *
 * LZ4's token is four bits each way, which is our even split exactly, so this
 * axis changes no constant here -- it exists so the two formats can diverge
 * where their guarantees do without either one's timings moving because of the
 * other's. LZ4 promises a block ends in a literal run of at least five bytes
 * with no match inside the last twelve; our packer emits matches up to the
 * final byte. Code that may assume the first belongs under KEVA_LZ4 and nowhere
 * else.
 *
 * The rule this axis is here to keep: our own format's body is not edited to
 * make foreign blocks faster. A separate body cannot change what our own values
 * execute, which is the only way to promise that and mean it.
 */
#if defined(KEVA_LZ4) && defined(KEVA_WIDE)
#  error "LZ4 has one token layout and it is the even split"
#endif

#if defined(KEVA_LZ4)
#  if defined(KEVA_XEON)
#    define KEVA_PFX   lx
#    define KEVA_ENTRY keva_unpack_lz4_xeon
#  elif defined(KEVA_EPYC)
#    define KEVA_PFX   lp
#    define KEVA_ENTRY keva_unpack_lz4_epyc
#  elif defined(KEVA_SSSE3)
#    define KEVA_PFX   ls
#    define KEVA_ENTRY keva_unpack_lz4_ssse3
#  else
#    define KEVA_PFX   l
#    define KEVA_ENTRY keva_unpack_lz4
#  endif
#  define LIT_SHIFT  4
#  define LIT_MAX    15
#  define MAT_MAX    15
#elif defined(KEVA_WIDE)
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
/*
 * Two for the literal run, five for the match, one for "the offset the last
 * block used". Was two and six.
 *
 * This is the body records are decoded under -- they reach this split after
 * fifteen blocks and stay, 1552 of records_64k's 1567 -- and the bit is free
 * here: 31.8% of records_64k's offsets repeat the one before and 72.8% of
 * records_4k's, while a five-bit match field moves the share of blocks needing
 * a chain by 0.0 and 1.2 percentage points. Wide match lengths are bimodal, so
 * what passes 31 almost always passes 63 too and was paying for a chain either
 * way. In the even split the same bit cost 22.9%. See docs/MEASUREMENTS.md.
 */
#  define LIT_SHIFT  6
#  define LIT_MAX    3
#  define MAT_SHIFT  1
#  define MAT_BITS   5
#  define MAT_MAX    31
#  define KEVA_REPEAT 1
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
/*
 * Four bits each, which is also LZ4's token.
 *
 * The repeat bit was taken out of the literal field here and put back: it cost
 * 22.9% on records_512 and 21.5% on varied_512, because the share of blocks
 * whose literal length needs an extension chain doubled and a saturated literal
 * length is the one thing that leaves the hot path. It lives in the wide body
 * now, where the field it costs is the one whose overflow stays home.
 */
#  define LIT_SHIFT  4
#  define LIT_MAX    15
#  define MAT_MAX    15
#endif

/* The even split and the LZ4 body spend all eight token bits on lengths. */
#ifndef MAT_SHIFT
#  define MAT_SHIFT  0
#endif
#ifndef MAT_BITS
#  define MAT_BITS   4
#endif

/*
 * Two hundred and fifty-six bits for the final literal run, where the part is
 * known to have them.
 *
 * The copy loop is sixty-one percent of the kernel on noise_512 and is already
 * down to five micro-ops a block, so there are no instructions left to remove
 * from it -- its IPC inside the loop is 3.1, well under the issue width, which
 * says it is waiting on ports rather than on issue. Two stores per thirty-two
 * bytes is the wall, and the only way through it is to move more per store.
 *
 * Only where the part line is known and only for foreign blocks. `vmovdqu ymm`
 * is AVX, which every Xeon and EPYC since 2011 has -- but "every" is an
 * assumption about a brand string, so the dispatch asks CPUID for it as well
 * and hands a part without it the SSSE3 body. Our own format's bodies are not
 * touched: the promise attached to them is that they do not change.
 *
 * 256 and not 512: AVX-512 is fused off on client parts and downclocks older
 * server ones, and this loop is bound by store ports rather than by width, so
 * the wider registers would buy a fraction of what they risk.
 */
#if defined(KEVA_LZ4) && (defined(KEVA_XEON) || defined(KEVA_EPYC))
#  define KEVA_YMM 1
#  define LIT_BLK  64
#else
#  define LIT_BLK  32
#endif

#define MIN_MATCH         4
#define OVERRUN           32

/*
 * Bytes the fast path stores for the literal run before it knows the length.
 *
 * Sixteen was right while both bodies could carry fourteen literals. The wide
 * split carries three, so it stored a vector where a dword would do -- and paid
 * for the reach twice, once here and once in FAST_IN below, which is where it
 * hurts: past that margin every block runs on the checked path at three to four
 * times the price. On AArch64 the same change was worth 5.6% on records_4k,
 * where a packed value is 5.1 bytes to a block and the margin was costing 3.4
 * of them. See docs/MEASUREMENTS.md.
 */
#if LIT_MAX <= 3
#  define FAST_LITERAL 4
#else
#  define FAST_LITERAL 16
#endif
#define MAT_CAP           (MAT_MAX + MIN_MATCH)

/*
 * How much the fixed match move copies: one vector pair under the even split,
 * three under the wide one, because that is what covers a match the split can
 * carry inline. Without it the wide split would send every match past
 * twenty-eight bytes to the checked path, which is the shape it exists for.
 */
#if MAT_CAP > 64
#  define COPY_MAX   96
#elif MAT_CAP > 48
#  define COPY_MAX   64
#elif MAT_CAP > 32
#  define COPY_MAX   48
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

/*
 * Input the fast path may read past the cursor: the token and the literal
 * store. The offset needs no room of its own -- this path only runs while the
 * literal length is below LIT_MAX, so the run ends at least two bytes inside
 * the store and the offset sits in the same reach.
 */
#define FAST_IN (1 + FAST_LITERAL)

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
/*
 * Sixty-four rather than sixteen, and the reason is a measurement rather than
 * a rule of thumb.
 *
 * Adding the LZ4 bodies put about sixteen kilobytes of text in front of some of
 * these symbols and changed nothing else -- every one of our own bodies is
 * byte-identical to what it was, compared in the text section. An EPYC 9B14
 * still lost 6.6% on records_64k and 4.8% on records_4k, measured A-B-B-A on
 * one machine so that neither the machine nor the ordering could be the cause:
 *
 *   records_64k   12.106  11.068  11.308  11.843   (before, after, after, before)
 *
 * The two groups do not overlap. What moved was the address, so the address is
 * what has to stop moving. At sixteen a symbol lands anywhere in a 64-byte
 * line depending on what precedes it; at sixty-four it lands at the start of
 * one, and the aligned loop head inside it keeps a fixed offset from there.
 *
 * Zen is where this showed up. The same shift cost 1.7% and 0.8% on a Xeon and
 * nothing measurable on Axion.
 *
 * The cost is up to 48 bytes of padding per symbol, and there are thirteen.
 */
    .p2align 6
    .globl KEVA_NAME(KEVA_ENTRY)

KEVA_NAME(KEVA_ENTRY):
    push    %rbx
    push    %rbp
    push    %r12
    push    %r13
    push    %r14
    push    %r15
    /*
     * Sixty-four zeroed bytes for the tail of the input as well. See
     * L(src_edge): the last bytes of a value are copied there so the fast loop
     * can read its fixed reach past them without leaving the buffer.
     */
#ifdef KEVA_REPEAT
    /*
     * Sixteen bytes for the repeat code: the offset the last block wrote, and
     * the token's low bit. All fifteen registers are spoken for, and the red
     * zone is not something to lean on -- this is tested under a binary
     * translator, which is free to use the space below the stack pointer.
     *
     *   (%rsp)   the token byte of the block being read
     *   8(%rsp)  the offset to reuse where its low bit is set
     */
    sub     $80, %rsp
    movq    $0, 8(%rsp)
#  define PAD_OFF 16
#else
    sub     $64, %rsp
#  define PAD_OFF 0
#endif

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

#ifdef KEVA_LZ4
    /*
     * A block that is one literal run and nothing else -- how LZ4 encodes data
     * that does not compress -- is recognised before the loop is entered.
     *
     * Only the question is asked here; the answering is out of line below. Not
     * for tidiness: the loop head is `.p2align 5`, so the padding in front of it
     * absorbs a few bytes added here and `L(block)` keeps the address it had.
     * The first cut put all thirty instructions inline, moved the loop head by
     * a hundred bytes, and paid 11.6% of records_512 on an EPYC and 6.3% on a
     * Xeon for a case those values never take.
     *
     * The test itself: a block that is one run has nothing behind that run, so
     * LZ4 writes zero in the token's match field. A block that compresses has a
     * match and says so. It does not decide alone -- a match of exactly
     * MIN_MATCH writes zero too -- which is what the equalities below are for.
     */
    movzbl  (%r14), %eax
    test    $MAT_MAX, %eax
    je      L(lit_only)
L(lit_only_back):
#endif

/*
 * The loop head is aligned because the micro-op cache is fed in 32-byte
 * windows and a hot loop that straddles one is delivered from the slower
 * decoders instead. It costs a few bytes of padding once.
 *
 * Sixty-four rather than thirty-two, and for the same reason the entry carries
 * it: at thirty-two, twelve bytes added in front of this -- a token load, a
 * test and a branch -- push it over a boundary and it moves a whole window.
 * Measured, that is 12.6% of records_512 on an EPYC and 10.5% on a Xeon, for
 * three instructions those values execute once. At sixty-four the padding
 * absorbs the change and the address does not move.
 */
    .p2align 6
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
#ifndef KEVA_WIDE
    /*
     * And the three bytes after it, in a register that nothing waits on.
     *
     * A block with no literals puts its offset at the two bytes past the token
     * and its match length's extension at the byte past those, so one dword at
     * the same address carries all three and saves two dependent L1 accesses:
     * the offset's address is the token shifted, and the extension byte's is a
     * load behind that.
     *
     * Loading the dword *instead* of the byte was measured on a Xeon and cost
     * varied_512 sixteen percent: it forced the literal nibble to be masked
     * again, and `perf annotate` put 3.49% of the kernel on that one `and`.
     * Two loads from one cache line are cheaper than one load and a mask,
     * because this one is on nobody's chain.
     */
    mov     (%r14), %edx                    /* offset and length byte, for later */
    /*
     * One compare, and it predicts on every shape.
     *
     * A token of exactly 0x0F says two things at once: no literals, and a
     * saturated match. The census puts both together on two thirds of
     * records_64k's blocks -- 88% carry no literals and 75% saturate -- and on
     * almost none of records_512's, whose blocks carry one to three literals.
     * So the branch is taken two thirds of the time on one shape and never on
     * the other, and predicted on both.
     *
     * Testing the literal nibble alone was measured instead and cost
     * records_512 nine percent: there it is a coin flip, and `perf annotate`
     * found the cost spread across the loop rather than on any instruction.
     */
#endif

    /* ---- fast path -------------------------------------------------- */

    cmp     %rbp, %rdi
    ja      L(dst_edge)                     /* too near the end to read ahead */

#ifndef KEVA_WIDE
    /*
     * After the output zone test, not before it: L(fast_zsat) writes, and the
     * zone is what says it may.
     */
    cmp     $0x0F, %eax
    je      L(fast_zsat)
#endif

#ifdef KEVA_REPEAT
    /*
     * The token's low bit, parked where every path can reach it.
     *
     * It cannot stay in the token: the long-literal path overwrites `eax`
     * walking its length chain, and it cannot ride in `r11d` beside the match
     * field without breaking the comparisons against MAT_MAX. All fifteen
     * registers are spoken for and this function calls nothing, so the red zone
     * below the stack pointer is ours -- a byte store per block, a byte test
     * where it is read.
     */
    mov     %al, (%rsp)
#endif
    mov     %eax, %r10d
    shr     $LIT_SHIFT, %r10d               /* short literal */
    mov     %eax, %r11d
#if MAT_SHIFT
    shr     $MAT_SHIFT, %r11d
#endif
    and     $MAT_MAX, %r11d                 /* short match */
    lea     1(%r14), %rsi                   /* where the literals begin */
    cmp     $LIT_MAX, %r10d
    je      L(fast_litext)

#ifdef KEVA_REPEAT
    /*
     * The offset, unless the token's low bit says it is the one from the block
     * before. Where it does there is no load and no address to compute, and the
     * match can start as soon as its length is known -- two links out of the
     * chain `token -> literal length -> offset address -> offset`.
     *
     * Kept below the stack pointer rather than in a register: all fifteen are
     * spoken for, and this function calls nothing, so the red zone is ours. A
     * store on a fresh offset, a load on a repeat, both aligned and both in L1.
     */
    testb   $1, (%rsp)
    jne     L(fast_same)
    movzwl  (%rsi,%r10,1), %ecx             /* offset */
    mov     %rcx, 8(%rsp)
    jmp     L(fast_offdone)
L(fast_same):
    mov     8(%rsp), %rcx
L(fast_offdone):
#else
    movzwl  (%rsi,%r10,1), %ecx             /* offset */
#endif
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
#if FAST_LITERAL == 4
    mov     (%rsi), %eax
    mov     %eax, (%rdi)
#else
    movdqu  (%rsi), %xmm0
    movdqu  %xmm0, (%rdi)
#endif

L(fast_litdone):
#ifdef KEVA_REPEAT
    lea     (%rsi,%r10,1), %r14             /* past the literals */
    testb   $1, (%rsp)
    jne     L(fast_cursor_done)
    add     $2, %r14                        /* and past the offset it wrote */
L(fast_cursor_done):
#else
    lea     2(%rsi,%r10,1), %r14            /* past the literals and the offset */
#endif

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
#if COPY_MAX > 48
    movdqu  32(%rsi), %xmm2
    movdqu  48(%rsi), %xmm3
    movdqu  %xmm2, 32(%r8)
    movdqu  %xmm3, 48(%r8)
#elif COPY_MAX > 32
    /* One register, not two: the wide split carries 35 bytes inline since the
     * repeat bit took a match bit, so the fourth sixteen was written for
     * nothing -- on the body records spend 99% of their blocks in. */
    movdqu  32(%rsi), %xmm2
    movdqu  %xmm2, 32(%r8)
#endif
#if COPY_MAX > 64
    movdqu  64(%rsi), %xmm4
    movdqu  80(%rsi), %xmm5
    movdqu  %xmm4, 64(%r8)
    movdqu  %xmm5, 80(%r8)
#endif
    mov     %rax, %rdi
    jmp     L(block)

#ifndef KEVA_WIDE
/*
 * A token of exactly 0x0F: no literals, a saturated match, and everything the
 * block needs already in `edx` from the loop head.
 *
 * Nothing is read that was not already loaded. The offset sits at the two bytes
 * past the token and the length's extension at the byte past those, so one
 * dword covers all three -- where the ordinary path pays two dependent L1
 * accesses, one to reach the offset and one behind it to reach the extension.
 * The cursor steps by a constant four.
 */
L(fast_zsat):
    mov     %rdi, %r8                       /* nothing written before the match */
    shr     $8, %edx
    movzwl  %dx, %ecx                       /* the offset */
    shr     $16, %edx                       /* the extension byte */
    mov     %r8, %r10
    sub     %rbx, %r10                      /* available */
    lea     -1(%rcx), %rax
    cmp     %r10, %rax
    jae     L(fail)                          /* zero, or reaching before the output */
    lea     MIN_MATCH+MAT_MAX(%rdx), %r9    /* 4 + 15 + what the byte adds */
    add     $4, %r14                        /* token, offset, extension */
    cmp     $255, %edx
    jne     L(fast_matgo)
    jmp     L(fast_matchain)                 /* 274 bytes or more: the chain goes on */
#endif

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

    /*
     * Past the token's own width the entry guard covers nothing, so the run
     * and the offset behind it are bounded here.
     *
     * Not reaching means one of two things, and only one of them is an error.
     * The run may be the last one in the value, which has no offset behind it
     * at all -- that is how a stream ends. Or the stream is truncated. This
     * cannot tell them apart cheaply and used to call both a failure, which
     * refused every block liblz4 writes for data that does not compress: those
     * are a single literal run covering the whole value, and it ends exactly
     * where the offset would have been read.
     *
     * The entry guard covers the short form of this -- a final run of under
     * LIT_MAX bytes cannot reach the fast path, because FAST_IN demands more
     * bytes than such a block contains. It cannot cover this one, since a run
     * behind an extension chain has no bounded length.
     *
     * So hand the block to the checked path, which decodes a final run
     * correctly and reports a truncated one as the error it is. `r14` still
     * points at the token -- only `rsi` moved -- and the token itself is
     * reloaded because the chain above needed `rax`.
     */
    lea     2(%rsi,%r10,1), %rax
    cmp     %r13, %rax
    ja      L(fast_litend)
    lea     (%rdi,%r10,1), %r8              /* dst after the literals */
    cmp     %r12, %r8
    ja      L(fail)

#ifdef KEVA_REPEAT
    testb   $1, (%rsp)
    jne     L(fastlit_same)
    movzwl  (%rsi,%r10,1), %ecx             /* offset */
    mov     %rcx, 8(%rsp)
    jmp     L(fastlit_offdone)
L(fastlit_same):
    mov     8(%rsp), %rcx
L(fastlit_offdone):
#else
    movzwl  (%rsi,%r10,1), %ecx             /* offset */
#endif
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

    /*
     * The last bytes of the input, where the fast loop may read past them.
     *
     * The fast path reads a fixed distance from the cursor -- a token and the
     * literal move -- and stops that far short of the end rather than test per
     * block. On a long value that margin is nothing; on a short one it is most
     * of the last blocks. records_192 packs to 134 bytes, so seventeen of them
     * are 13% of the stream on a path three to four times dearer, and the same
     * change on aarch64 measured +8.1% on exactly that shape.
     *
     * So the remainder is copied somewhere with room after it: sixty-four
     * zeroed bytes in this frame. `r14` then walks the copy, `r13` ends it and
     * `r15` becomes that same end -- no margin, because there is nothing left
     * to read past. Nothing else in the body knows: the match copies read the
     * output, not the input.
     *
     * Padding cannot hide a truncated stream. Any block reading a byte at or
     * past the end leaves the cursor past it, a zero offset is refused where it
     * is read, and L(done) checks the cursor landed exactly on the end.
     *
     * `r15 == r13` says this has already happened, and the second visit falls
     * through to the checked path as before.
     */
    cmp     %r13, %r15
    je      L(src_slow)

    mov     %r13, %rdx
    sub     %r14, %rdx                      /* what is left, one to FAST_IN */
    lea     PAD_OFF(%rsp), %rcx
    pxor    %xmm0, %xmm0
    movdqu  %xmm0, (%rcx)
    movdqu  %xmm0, 16(%rcx)
    movdqu  %xmm0, 32(%rcx)
    movdqu  %xmm0, 48(%rcx)
    mov     %rcx, %r9
    mov     %r14, %rsi
    /*
     * A ladder rather than a loop: seventeen bytes at most, and only the bits
     * that are set cost anything. A byte at a time measured 6.5% of a
     * 512-byte value on aarch64, which was most of what the copy was to buy.
     */
    test    $16, %dl
    je      L(src_pad8)
    movdqu  (%rsi), %xmm0
    movdqu  %xmm0, (%r9)
    add     $16, %rsi
    add     $16, %r9
L(src_pad8):
    test    $8, %dl
    je      L(src_pad4)
    mov     (%rsi), %rax
    mov     %rax, (%r9)
    add     $8, %rsi
    add     $8, %r9
L(src_pad4):
    test    $4, %dl
    je      L(src_pad2)
    mov     (%rsi), %eax
    mov     %eax, (%r9)
    add     $4, %rsi
    add     $4, %r9
L(src_pad2):
    test    $2, %dl
    je      L(src_pad1)
    movzwl  (%rsi), %eax
    mov     %ax, (%r9)
    add     $2, %rsi
    add     $2, %r9
L(src_pad1):
    test    $1, %dl
    je      L(src_pad_done)
    movzbl  (%rsi), %eax
    mov     %al, (%r9)
L(src_pad_done):
    mov     %rcx, %r14                      /* the cursor walks the copy */
    lea     (%rcx,%rdx,1), %r13             /* which ends here */
    mov     %r13, %r15                      /* and the zone reaches all of it */
    jmp     L(block)

L(src_slow):
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

/*
 * A literal run in the fast path with no room for an offset behind it. Either
 * the value's last run or a truncated stream; the checked path knows which.
 * See the note at the test that sends blocks here.
 */
L(fast_litend):
#ifdef KEVA_LZ4
    /*
     * The run reaches the end of the input, which is what the last literal run
     * of a value does -- and that is not a reason to ask anything twice.
     *
     * Falling through to L(slow) reloads the token, extracts the nibble again
     * and walks the same length chain a second time. On a value that is one
     * block, which 512 bytes of incompressible data is exactly, those are two
     * more dependent L1 loads with no second block to overlap them against, on
     * a decode that measures about thirty-six cycles in total against liblz4's
     * twenty-five.
     *
     * Everything the copy wants is already here: `rsi` past the length bytes,
     * `rdi` at the output, `r10` the length. Missing are the two bounds the
     * fast path had not reached yet, and `r14`, which L(lit_done) advances by
     * the run -- so it has to point at the run and not at the token.
     */
    lea     (%rsi,%r10,1), %rax
    cmp     %r13, %rax                      /* the run ends inside the input */
    ja      L(fast_litend_slow)
    lea     (%rdi,%r10,1), %r9              /* and inside the output */
    cmp     %r12, %r9
    ja      L(fast_litend_slow)
    test    %r10, %r10                      /* L(lit_last) is not written for nought */
    je      L(fast_litend_slow)
    mov     %rsi, %r14
    jmp     L(lit_last)
L(fast_litend_slow):
#endif
    movzbl  (%r14), %eax
    jmp     L(slow)

    /* ---- exact path ------------------------------------------------- */

L(slow):
    inc     %r14
#ifdef KEVA_REPEAT
    /* Every entry to this path reloads the token, so the bit is parked again
     * here -- the fast path's copy belongs to whichever block wrote it. */
    mov     %al, (%rsp)
#endif
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
#ifdef KEVA_LZ4
    ja      L(lit_last)
#else
    ja      L(lit_bytes)
#endif

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

#ifdef KEVA_LZ4
/*
 * The last literal run, wide for as long as it is wide.
 *
 * The test above asks whether the run *ends* more than a block short of the
 * input, and where it does not it used to send the whole run to the ladder --
 * however long the run was. For an incompressible value that run is the whole
 * value: liblz4 encodes 64 KiB of noise as one run ending exactly at the end of
 * the input, so all 65536 bytes went sixteen at a time because the last thirty-
 * two needed care. Four thousand iterations where two thousand would do, and it
 * is the whole of the noise column:
 *
 *   noise_512   Xeon 0.30x   EPYC 0.49x   Axion 0.84x   M2 0.81x
 *   noise_4k         0.81x        0.55x         0.65x      0.88x
 *   noise_64k        0.95x        0.89x         0.62x      0.80x
 *
 * The bound belongs on the iteration, not on the run. This loop reads only
 * bytes it is going to write, so it can run to within a block of the end with
 * no guard at all -- `rcx` is what is left of the run, and thirty-two bytes are
 * read only while thirty-two remain. What is left after it is under a block,
 * and that is what the ladder below is for.
 *
 * Which also settles the Xeon question underneath. `rep movsb` was measured at
 * +8.9% on a run of a dozen bytes and then reached with runs of 65536, and its
 * noise column reads 0.30x, 0.81x, 0.95x -- a start-up cost divided by a
 * growing payload. Coming through here it only ever sees the remainder, which
 * is the size it was measured on. The axis was run length all along, not the
 * part line.
 *
 * Under KEVA_LZ4 only. Our own format's last run is short, because the value in
 * front of it compressed, so this would be paying a test to help a case that
 * does not arise -- and the promise attached to that body is that it does not
 * change.
 */
L(lit_last):
    mov     %r10, %rcx
#ifdef KEVA_LZ4
    /*
     * The platform's memcpy, which is what liblz4 calls here.
     *
     * A 512-byte incompressible value is one block: token, two length bytes,
     * 512 literals. Everything is on one dependency chain, and the copy that
     * ends it measured 31 cycles against liblz4's 25 on a Xeon 8481C. The
     * difference is not the loop below -- eight iterations of sixty-four bytes
     * is about ten cycles -- it is that liblz4 hands this run to memcpy, which
     * on glibc dispatches into a tuned AVX entry with a jump table for exactly
     * these sizes.
     *
     * The same call is worth +34% on aarch64 for 64 KiB runs, where it was
     * measured against a threshold rather than guessed. Here the threshold is
     * lower because the loop is not what wins: 128 bytes is four iterations,
     * below which a call cannot pay for itself.
     *
     * Once per value. The registers it destroys are `r9` and `r10`, which the
     * tail wants; `r14` and `r12` are callee-saved and survive. The stack moves
     * by twenty-four rather than sixteen so the call lands on a 16-byte
     * boundary -- six pushes leave it eight short.
     */
#  define MEMCPY_MIN 128
    cmp     $MEMCPY_MIN, %rcx
    jb      L(lit_noncall)
    sub     $24, %rsp
    mov     %r9, (%rsp)
    mov     %r10, 8(%rsp)
    mov     %rcx, %rdx                      /* memcpy(rdi = dst, rsi = src, rdx = n) */
    call    KEVA_SYM(memcpy)
    mov     (%rsp), %r9
    mov     8(%rsp), %r10
    add     $24, %rsp
    jmp     L(lit_done)
L(lit_noncall):
#endif
#ifdef KEVA_XEON
    /*
     * ...and on a part with Fast Short REP MOV the string instruction is still
     * the right answer once the run is long enough to amortise starting it.
     *
     * The first cut of this loop removed `rep movsb` from the LZ4 body outright,
     * on the reasoning that nothing long would reach it any more. That was an
     * assumption, and the measurement refused it -- three run lengths on a Xeon
     * 8481C, GiB/s on the whole value:
     *
     *   512 B     rep movsb 16.1   this loop 21.8
     *   4096 B              59.0             30.6
     *   65536 B             31.9             31.6
     *
     * Two regimes and a crossover. `rep movsb` costs about twenty-two cycles
     * before it moves a byte and then sustains 23 B/cycle; this loop starts
     * immediately and sustains 11.9. They meet near 550 bytes, and past 64 KiB
     * both are held up by memory instead.
     *
     * So the threshold sits above the crossover with room on either side: a
     * 512-byte run takes the loop, which measured 35% better, and a 4 KiB run
     * takes the string instruction, which measured 93% better. Picking either
     * one for both loses half the column.
     *
     * The axis really is run length. The part line decides *whether* there is a
     * crossover -- Zen has ERMSB without FSRM and pays tens of cycles on the
     * same copy, so its body has no such branch -- but it does not decide where.
     */
#  define REP_MIN 1024
    cmp     $REP_MIN, %rcx
    jae     L(lit_rest)
#endif
    /*
     * Past the string instruction, not into it.
     *
     * What is left below is under a block -- nought to thirty-one bytes -- and
     * the threshold above says a run that short is the ladder's business. Going
     * through L(lit_rest) would run `rep movsb` on it, and on a 512-byte value,
     * which is exactly sixteen blocks, it runs it on *nothing*: the loop leaves
     * the remainder at zero and the microcode start-up is paid in full to copy
     * no bytes. It was the hottest instruction in the kernel, at 18.65%.
     */
    cmp     $LIT_BLK, %rcx
    jb      L(lit_b16)

    /*
     * A negative index counting up to zero, rather than two cursors and a
     * counter.
     *
     * The first version advanced `rsi`, advanced `rdi`, lowered `rcx`, compared
     * it and jumped back: five instructions of bookkeeping around four that
     * copy. At IPC 5.37 against an issue width of six that is not overhead, it
     * is the cost. `perf annotate`, noise_512, the loop being 57.9% of the
     * kernel:
     *
     *    6.11%  cmp    $0x20,%rcx        10.98%  movdqu %xmm1,0x10(%rdi)
     *    6.36%  movdqu (%rsi),%xmm0       2.67%  add    $0x20,%rsi
     *    7.00%  movdqu 0x10(%rsi),%xmm1   7.36%  add    $0x20,%rdi
     *    5.64%  movdqu %xmm0,(%rdi)       3.44%  sub    $0x20,%rcx
     *                                     8.29%  jmp
     *
     * Both cursors move in the addressing mode instead, and the loop ends when
     * the index reaches zero, which is the flag `add` already set. Five
     * bookkeeping instructions become one fused pair: ten per block become six,
     * and nine micro-ops become five.
     */
    mov     %rcx, %rax
    and     $-LIT_BLK, %rax                 /* whole blocks of it */
    add     %rax, %rsi
    add     %rax, %rdi
    sub     %rax, %rcx                      /* and what the ladder gets */
    neg     %rax
L(lit_l32):
#ifdef KEVA_YMM
    vmovdqu (%rsi,%rax,1), %ymm0
    vmovdqu 32(%rsi,%rax,1), %ymm1
    vmovdqu %ymm0, (%rdi,%rax,1)
    vmovdqu %ymm1, 32(%rdi,%rax,1)
#else
    movdqu  (%rsi,%rax,1), %xmm0
    movdqu  16(%rsi,%rax,1), %xmm1
    movdqu  %xmm0, (%rdi,%rax,1)
    movdqu  %xmm1, 16(%rdi,%rax,1)
#endif
    add     $LIT_BLK, %rax
    jne     L(lit_l32)
#ifdef KEVA_YMM
    /*
     * Back to legacy width before anything else runs. Everything around this
     * loop is SSE, and mixing 256-bit VEX with legacy SSE in one function costs
     * a transition on every Intel part that has both. One instruction, once per
     * value, against a stall on every `movdqu` after it.
     */
    vzeroupper
#endif
    jmp     L(lit_b16)
#endif

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
/* Entered with `rcx` already holding what is left, from the wide loop above. */
L(lit_rest):
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

#ifdef KEVA_REPEAT
    testb   $1, (%rsp)
    jne     L(slow_same)
#endif
    lea     2(%r14), %rax
    cmp     %r13, %rax
    ja      L(fail)
    movzwl  (%r14), %ecx                    /* offset */
    mov     %rax, %r14
#ifdef KEVA_REPEAT
    mov     %rcx, 8(%rsp)
    jmp     L(slow_offdone)
L(slow_same):
    mov     8(%rsp), %rcx                  /* no bytes were written for it */
L(slow_offdone):
#endif

    mov     %r11d, %r10d
#if MAT_SHIFT
    shr     $MAT_SHIFT, %r10d
#endif
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
    /* Past the end means the stream claimed bytes it does not have, and the
     * padded copy is where it would have looked for them. */
    cmp     %r13, %r14
    ja      L(fail)
    mov     %r12, %rax
    sub     %rbx, %rax
    jmp     L(ret)

L(fail):
    xor     %eax, %eax

L(ret):
#ifdef KEVA_REPEAT
    add     $80, %rsp
#else
    add     $64, %rsp
#endif
    pop     %r15
    pop     %r14
    pop     %r13
    pop     %r12
    pop     %rbp
    pop     %rbx
    ret

#ifdef KEVA_LZ4
/*
 * Is this block one literal run, exactly filling the output?
 *
 * Two exact equalities: the run ends precisely at the end of the input, and it
 * produces precisely the declared length. Anything else goes back to the loop,
 * which is unchanged.
 *
 * The chain needs no bound of its own -- each turn advances the cursor by one
 * and adds at most 255, so it runs out of input long before the length matters,
 * and the equalities reject whatever it reached.
 *
 * This can refuse to fire; it cannot fire wrongly. Every way of getting it
 * wrong sends the block back to the loop, which is why no mutation of it fails
 * a test, and why only a measurement can show that it fires at all.
 */
L(lit_only):
    /* Reloaded rather than carried, so this reads as one piece. Cold. */
    movzbl  (%r14), %eax                    /* token */
    lea     1(%r14), %rsi                   /* where the literals would begin */
    mov     %eax, %r10d
    shr     $LIT_SHIFT, %r10d
    cmp     $LIT_MAX, %r10d
    jne     L(lit_only_ends)

    xor     %r10, %r10
L(lit_only_chain):
    cmp     %r13, %rsi
    jae     L(lit_only_back)
    movzbl  (%rsi), %edx
    inc     %rsi
    add     %rdx, %r10
    cmp     $255, %edx
    je      L(lit_only_chain)
    add     $LIT_MAX, %r10

L(lit_only_ends):
    lea     (%rsi,%r10,1), %rax
    cmp     %r13, %rax                      /* ends exactly at the input's end */
    jne     L(lit_only_back)
    lea     (%rdi,%r10,1), %r9
    cmp     %r12, %r9                       /* and fills the output exactly */
    jne     L(lit_only_back)
    test    %r10, %r10
    je      L(lit_only_back)
    mov     %rsi, %r14
    jmp     L(lit_last)
#endif
