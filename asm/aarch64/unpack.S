/*
 * keva_unpack -- the decoder, one call per value.
 *
 *   uint32_t keva_unpack(const uint8_t *src, uintptr_t src_len,
 *                        uint8_t *dst, uintptr_t dst_cap,
 *                        uintptr_t declared, uintptr_t start);
 *
 * `start` is where in `dst` this call begins producing, and it is zero for a
 * value that uses one token split throughout. A value that switches split
 * partway is decoded by two calls: the first produces [0, switch) with one
 * kernel, the second [switch, declared) with the other, and the second is
 * handed the same `dst` so its backward references reach into what the first
 * wrote. Nothing else in the loop changes -- the zone limits already derive
 * from `declared`, and the end test already compares against it.
 *
 * `src` points past the length header, which the caller has already decoded
 * into `declared`. Returns `declared` on success, or 0 to mean "this needs the
 * portable decoder" -- either the stream is malformed, or the buffer is too
 * small for the checks below to be sufficient. The caller then runs the Rust
 * decoder, which produces the precise error. A stream only fails when it is
 * corrupt, so paying for that twice costs nothing that matters, and it keeps
 * every error message in one place.
 *
 * AAPCS64: x0 src, x1 src_len, x2 dst, x3 dst_cap, x4 declared, x5 start.
 * Result in w0.
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
 * per block, `L(entry)` derives the cursor at which it stops holding and the
 * loop compares against it. Past that point the tail runs on the exact path.
 *
 *   OVERRUN 32   FAST_LITERAL 16   MIN_MATCH 4   MAX_UNPACKED 512 MiB
 */

#include "../common.h"

/*
 * Two specialisations, one body.
 *
 * The token's bits are divided one way for most data and another for data that
 * compresses well, and which one a value uses is in its header. Assembly does
 * not take that as a parameter -- it takes it as a second copy of the code,
 * because a register holding the shift would sit on the critical path of every
 * block for the whole length of the value.
 *
 * So this file is assembled twice, once per split, with every local label run
 * through L() to keep the two sets apart. build.rs does the second pass with
 * -DKEVA_WIDE.
 *
 *   even (LZ4's own, and what a block liblz4 wrote uses)
 *       four bits each: literal runs to 14, match lengths to 18
 *   wide
 *       two and six: literal runs to 2, match lengths to 66
 *
 * The wide one exists because counting real blocks says the match length
 * overflows four bits in 99% of them on data that compresses six-fold. Six bits
 * take that to zero.
 */
/*
 * And one body per format. LZ4's token is four bits each way, which is the even
 * split exactly, so this axis changes no constant -- it exists so the two
 * formats can diverge where their guarantees do without either one's timings
 * moving because of the other's. See the longer note in the x86-64 unpack.S.
 */
#if defined(KEVA_LZ4) && defined(KEVA_WIDE)
#  error "LZ4 has one token layout and it is the even split"
#endif

#if defined(KEVA_LZ4)
#  define KEVA_PFX   l
#  define KEVA_ENTRY keva_unpack_lz4
#  define LIT_SHIFT  4
#  define LIT_MAX    15
#  define MAT_MAX    15
#elif defined(KEVA_WIDE)
#  define KEVA_PFX   w
#  define KEVA_ENTRY keva_unpack_wide
#  define LIT_SHIFT  6
#  define LIT_MAX    3
#  define MAT_MAX    63
#  define MAT_CAP    67                 /* MAT_MAX + MIN_MATCH */
#else
#  define KEVA_PFX   e
#  define KEVA_ENTRY keva_unpack
#  define LIT_SHIFT  4
#  define LIT_MAX    15
#  define MAT_MAX    15
#  define MAT_CAP    19
#endif

/*
 * Bytes the fixed match move writes, which is a property of the code below and
 * not of the format: one vector pair under the even split, three under the wide
 * one, because that is what covers a match the split can carry inline.
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
 * Two things have to hold, and the margin used to be derived from neither. It
 * was LIT_MAX + MAT_CAP -- "what a block writes" -- which is 34 under the even
 * split and 70 under the wide one.
 *
 * The first is a bound: the widest thing a fast block writes is its literals
 * plus COPY_MAX, and it must land inside the 64 bytes of slack the caller
 * provides past `declared`. That gives LIT_MAX + COPY_MAX - 64, which is 35 for
 * the wide split and negative -- so no constraint at all -- for the even one.
 *
 * The second is not a bound but a heuristic. A value ends in a literal run with
 * no offset after it, and the fast path always reads an offset, so the loop has
 * to stop before that last block. It stops when `produced` reaches this margin
 * short of the end, which covers any final run at least this long. A longer one
 * is not a correctness problem -- the block decodes to nonsense, the length
 * check at the end fails, and the caller runs the portable decoder -- it is
 * simply slow, so the margin wants to be a comfortable upper bound on a final
 * literal run and no more.
 *
 * Sixteen over the bound is that. Every byte of it costs: at 512 bytes and 36
 * bytes to a block, the old 34 put a whole block on the checked path, which is
 * three times dearer, and a call's fixed cost is 41% of the work at that size.
 */
#if (LIT_MAX + COPY_MAX) > 64
#  define OUT_MARGIN (LIT_MAX + COPY_MAX - 64 + 16)
#else
#  define OUT_MARGIN 16
#endif

/*
 * KEVA_SYM pastes with ##, which means its argument is not expanded first --
 * KEVA_NAME(KEVA_ENTRY) would name the symbol after the macro rather than after
 * what it stands for. One more level fixes it, because an argument to a macro
 * that does not paste is expanded before substitution.
 */
#define KEVA_NAME1(n) KEVA_SYM(n)
#define KEVA_NAME(n) KEVA_NAME1(n)

#define L2(p, n) .L ## p ## _ ## n
#define L1(p, n) L2(p, n)
#define L(n) L1(KEVA_PFX, n)


    .text
    .p2align 4
    .globl KEVA_NAME(KEVA_ENTRY)

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
 *   L(exact)  x0, x1, x2, x4, x6      L(repeat)  x0-x7
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
KEVA_NAME(KEVA_ENTRY):
    /*
     * Ten registers saved, nine used: x27 appears nowhere but here and the
     * epilogue. Removing it was tried and was a bug.
     *
     * It is saved *paired* with x28, and x28 is the output zone limit, which
     * this loop writes on every call. Dropping the pair dropped x28's save with
     * it, and the kernel began clobbering its caller's x28.
     *
     * Nothing caught that. The tests compare the bytes that come out, and the
     * bytes were right: a kernel that declines returns zero, the caller runs
     * the portable decoder, and the answer is the same. A test written
     * afterwards that asserts the kernel *accepts* did not catch it either --
     * whether the clobber breaks anything depends on whether the caller
     * happened to be using x28, and in that test's caller it did not. The check
     * that does catch it is static, in keva-asm's test module: every
     * callee-saved register this file writes must appear in the prologue.
     *
     * There was nothing to win either way. Nine registers plus x29 and x30 is
     * eleven values and 88 bytes, which is a 96-byte frame with or without it.
     */
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
    // Before x5 is used as scratch below: it carries where this call starts
    // writing, which for a single-split value is zero.
    mov     x24, x5                     // produced

L(entry):
    // The one capacity check. Everything below leans on it.
    add     x5, x22, #64
    cmp     x3, x5
    b.lo    L(fail)

    mov     x23, #0                     // at

    /*
     * The zone the fast loop runs in, as two limits computed once.
     *
     * Inside it a block needs no bound test of its own, and the margins are
     * exactly what a block reads and writes rather than a round number.
     *
     * Input, 17: a token, at most fourteen literals and a two-byte offset. The
     * literal move reads sixteen bytes from just past the token, which ends at
     * the same place.
     *
     * Output, OUT_MARGIN: see the constant. It is what the fixed match move
     * needs to stay inside the caller's slack, plus enough that the closing
     * literal run lands on the checked path rather than the fast one.
     *
     * These were 48 and 64, and the difference is not cosmetic. A 4 KiB value
     * of data that compresses six-fold packs to 656 bytes, so a 48-byte input
     * margin is 7% of the stream: the last eight blocks of a hundred and eight
     * fell out of the zone into the checked path, which is three times dearer
     * per block. The profile put 31% of the kernel there for 0.9% of blocks
     * that genuinely needed it. At 64 KiB the same margin is half a percent,
     * which is why throughput was flat across sizes while the gap to liblz4
     * lived entirely at the small end.
     *
     * Both clamp to zero for a value too small to hold one block, which then
     * runs entirely on the checked loop.
     */
    // Seventeen whatever the split: the block reads a token and then sixteen
    // bytes of literals unconditionally, and the offset sits inside that reach.
    subs    x25, x20, #17               // input limit
    csel    x25, x25, xzr, hs
    subs    x28, x22, #OUT_MARGIN           // output limit
    csel    x28, x28, xzr, hs

#ifdef KEVA_LZ4
    /*
     * A block that is one literal run and nothing else, recognised before
     * either loop is entered. See the x86-64 body, where this was measured:
     * +13% to +22% on the noise cells on both vendors.
     *
     * Only the question is asked here and the answering is out of line, which
     * on x86 was worth ten percent of records_512 -- thirty instructions in
     * front of the loop head moved it across an alignment boundary. AArch64
     * has no such padding to overrun, but the reason to keep a cold path cold
     * holds anyway.
     *
     * A block that is one run has nothing behind it, so LZ4 writes zero in the
     * token's match field. A block that compresses has a match and says so.
     */
    ldrb    w4, [x19, x23]
    tst     w4, #MAT_MAX
    b.eq    L(lit_only)
L(lit_only_back):
#endif

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
L(fast):
    /*
     * Four instructions for the two zone tests, and they stay four.
     *
     * `cmp x23,x25 / ccmp x24,x28,#2,lo / b.hs` is the same question in three,
     * and it was built and measured rather than argued about this time: the
     * short block went from 35 instructions to 34, verified in the
     * disassembly, and throughput did not move -- varied_64k -1.7%,
     * records_64k +1.6%, and every shape on identical LZ4 blocks between -0.1%
     * and -1.5%. Contradictory signs at that size are noise.
     *
     * The counters say why, and it is worth more than the instruction. Cycles
     * per block stayed at 8.6 to 9.0 while instructions per block fell, so IPC
     * fell with them: 4.67 to 4.52 on varied_64k, 5.48 to 5.31 on records_64k.
     * The loop is not issue-bound at the margin -- there was already slack in
     * the issue width, and handing one instruction back does not buy a cycle.
     *
     * That kills the whole class. Between the 5-cycle latency floor of the
     * token recurrence and the ~7-cycle issue floor of 40 instructions, this
     * loop sits at 8.7, and neither model explains the rest. Shaving single
     * instructions does not reach it. What has reached it, twice, is fewer and
     * bigger blocks: the wide split and lazy matching each bought about 19%,
     * and both work on the numerator.
     */
    cmp     x23, x25
    b.hs    L(tail)
    cmp     x24, x28
    b.hs    L(tail)

    ldrb    w4, [x19, x23]              // token
    lsr     w5, w4, #LIT_SHIFT                  // literal nibble
    and     w26, w4, #MAT_MAX                // match nibble, kept across helpers

    /*
     * Only the literal nibble gates the block now.
     *
     * A saturated *match* nibble used to leave for the general writer, and on
     * data that compresses well that is 99% of blocks -- so almost nothing ever
     * used the zone, and every block re-asked the questions the zone had
     * already answered. The long match is handled below instead, inside the
     * loop, which is what liblz4 does and why it carries three block bodies.
     */
    /* ---- the common block -------------------------------------------- */
    add     x7, x23, #1                 // literals start here
    cmp     w5, #LIT_MAX
    b.eq    L(fast_litlong)

    add     x8, x7, x5                  // the offset follows the literals
    add     x16, x24, x5                // output position after them

    // A short run carries at most fourteen literals, so one 16-byte move covers
    // it whatever the length. Trimming would cost a branch to save nothing: the
    // extra bytes are overwritten by the match that follows.
    ldr     q0, [x19, x7]
    str     q0, [x21, x24]

L(fast_offset):
    ldrh    w11, [x19, x8]              // offset
    add     x23, x8, #2                 // cursor past it

    /*
     * This branch is the most expensive one in the loop and it stays.
     *
     * Counters put 4.0% of all cycles on data that compresses six-fold into
     * branch mispredicts, and this is where they are: the test is taken 26.7% of
     * the time there, close enough to a coin flip to beat the predictor, while
     * every other shape measures 0.0 to 0.1%. It is worth 0.36 cycles a block,
     * which is the whole of the remaining gap to liblz4 on that shape.
     *
     * The obvious removal was tried and measured. Read the extension byte
     * whether or not there is one, fold it in with `csel`, and branch instead on
     * whether the fixed move still covers the result -- which on that data is
     * never, since every long match there lands between 67 and 96 bytes and the
     * move already writes 96. Mispredicts fell from 4.0% to 0.6% exactly as
     * intended, and throughput fell with them: records_4k -9.0%, records_64k
     * -7.3%, varied_4k -18.0%, varied_64k -18.4%.
     *
     * The reason is the address. That byte sits at `at + 3 + literals`, so
     * loading it unconditionally puts a second dependent L1 load into every
     * block's chain -- token load, literal count, address, load -- where before
     * three quarters of blocks had none. Three cycles of latency to save 0.36 of
     * misprediction. The branch is cheaper than the load it would replace, and
     * that stays true for any scheme that reads the byte before knowing whether
     * it exists.
     */
    cmp     w26, #MAT_MAX
    b.eq    L(fast_long)

    add     x14, x26, #4                // match length
    add     x12, x16, x14               // and after the match

    // Only the offset is still worth asking about. A short block lays down at
    // most fourteen literals and eighteen match bytes, and the zone holds the
    // cursor sixty-four short of the declared length, so it cannot overrun.
    cbz     w11, L(fail)
    cmp     x11, x16
    b.hi    L(fail)                      // reaches back before the output

L(fast_copy):
    add     x13, x21, x16               // where the match is written
    sub     x15, x13, x11               // and where it is read from
    /*
     * Far enough back that the bytes this reads are all already written, which
     * is what makes a straight move correct -- and that is the offset against
     * the match length, not against the width of the move.
     *
     * The move covers the longest match this split can carry without an
     * extension: one vector pair under the even split, two under the wide one,
     * where a match runs to 66 bytes.
     */
    cmp     x11, x14
    b.lo    L(fast_near)

    ldp     q0, q1, [x15]
    stp     q0, q1, [x13]
#if MAT_CAP > 32
    ldp     q2, q3, [x15, #32]
    stp     q2, q3, [x13, #32]
#endif
#if MAT_CAP > 64
    ldp     q4, q5, [x15, #64]
    stp     q4, q5, [x13, #64]
#endif

    mov     x24, x12
    b       L(fast)

L(fast_near):
    mov     x0, x13
    mov     x1, x15
    mov     x2, x11
    mov     x3, x14
    bl      L(repeat)
    mov     x24, x12
    b       L(fast)

/*
 * A long literal run, still inside the loop.
 *
 * Half the blocks of data that compresses only two-fold saturate the *literal*
 * nibble -- 50.8% of them, against 0% saturating the match nibble. Sending
 * those to the checked path meant half of that shape never used the zone, the
 * same way all of the six-fold shape did not before the long match moved in
 * here.
 *
 * The trick is that one pair of tests puts the cursors back inside a zone. A
 * run of at most 269 bytes is read, and both cursors are then checked with the
 * margin a following block needs -- so everything after this proceeds exactly
 * as a short block does, with no further questions about ends or lengths. A
 * run longer than that is rare enough to leave to the checked path, which is
 * also where the multi-byte length encoding belongs.
 */
L(fast_litlong):
    add     x9, x23, #1
    ldrb    w10, [x19, x9]              // the zone covers this byte already
    cmp     w10, #255
    b.eq    L(slow)                      // a run past 269 bytes
    add     x12, x10, #LIT_MAX               // the length this run would have
    add     x7, x23, #2                 // and it starts past the length byte

    add     x9, x7, x12
    add     x9, x9, #32
    cmp     x9, x20
    b.hi    L(slow)                      // the run, or its block copy, runs off
    add     x16, x24, x12
    add     x9, x16, #32
    cmp     x9, x22
    b.hi    L(slow)                      // no room for a following block

    // Committed only here. Every exit above goes to the checked path, which
    // re-reads the token and expects the nibble still to say fifteen -- writing
    // the decoded length into x5 before those tests made it skip the length
    // byte and mis-frame the rest of the value.
    mov     x5, x12
    add     x8, x7, x5                  // where the offset sits
    add     x0, x21, x24
    add     x1, x19, x7
    mov     x9, #0
L(fast_litblk):
    ldp     q0, q1, [x1]
    stp     q0, q1, [x0]
    add     x9, x9, #32
    cmp     x9, x5
    b.hs    L(fast_offset)
    add     x1, x1, #32
    add     x0, x0, #32
    b       L(fast_litblk)

/*
 * A long match, still inside the loop.
 *
 * Unlike a short one this can run to any length, so the two things the zone
 * cannot promise are asked here and nowhere else: that the length bytes are
 * inside the input, and that the match does not carry the output past what was
 * declared. Everything about the literals stays covered by the zone.
 *
 * This is the block that data compressing six-fold is made of -- 99% of its
 * blocks saturate the match nibble, with an average match of 38 bytes against
 * 1.4 literals. Sending them to the general writer meant the zone bought
 * nothing on exactly the shape it was built for.
 */
L(fast_long):
    mov     x14, #MAT_MAX
    mov     x8, #0
L(fast_lenbyte):
    cmp     x23, x20
    b.hs    L(fail)
    ldrb    w9, [x19, x23]
    add     x23, x23, #1
    add     x8, x8, x9
    cmp     w9, #255
    b.ne    L(fast_lendone)
    mov     x10, #0x20000000            // MAX_UNPACKED, 512 MiB
    cmp     x8, x10
    b.hi    L(fail)
    b       L(fast_lenbyte)
L(fast_lendone):
    add     x14, x14, x8
    add     x14, x14, #4                // match length
    add     x12, x16, x14

    cbz     w11, L(fail)
    cmp     x11, x16
    b.hi    L(fail)
    cmp     x12, x22
    b.hi    L(fail)                      // an unbounded length can overrun

    add     x13, x21, x16
    sub     x15, x13, x11
    cmp     x11, #32
    b.lo    L(fast_near)

    // Long enough to be worth the block loop, and far enough back to use it.
    mov     x5, #0
L(fast_longblk):
    ldp     q0, q1, [x15]
    stp     q0, q1, [x13]
    add     x5, x5, #32
    cmp     x5, x14
    b.hs    L(fast_longout)
    add     x15, x15, #32
    add     x13, x13, #32
    b       L(fast_longblk)
L(fast_longout):
    mov     x24, x12
    b       L(fast)

    /*
     * Outside the zone: the same format, every question asked again.
     *
     * Reached once per value, when either cursor nears its buffer. It does not
     * hand back -- the general writer below returns to `L(fast)`, which re-tests
     * the zone and arrives here again. One test rather than two paths.
     */
L(tail):
    cmp     x24, x22
    b.hs    L(done)
    cmp     x23, x20
    b.hs    L(fail)
    ldrb    w4, [x19, x23]              // token
    lsr     w5, w4, #LIT_SHIFT
    and     w26, w4, #MAT_MAX

    /* ---- one block, whatever its lengths ----------------------------- */
L(slow):
    add     x23, x23, #1

    mov     x15, x5                     // literal length
    cmp     x5, #LIT_MAX
    b.ne    L(slow_lit_ready)
    /*
     * The extended length, read here rather than called.
     *
     * On data that compresses well this runs once per block, and a call for it
     * is a branch out, a branch back and a return-stack slot to move at most a
     * couple of bytes. liblz4 has it as a self-loop inside its decode loop for
     * the same reason.
     */
    mov     x8, #0
L(litlen_byte):
    cmp     x23, x20
    b.hs    L(fail)
    ldrb    w9, [x19, x23]
    add     x23, x23, #1
    add     x8, x8, x9
    cmp     w9, #255
    b.ne    L(litlen_done)
    // A run of 255s on a corrupt stream would otherwise spin until it wrapped.
    mov     x10, #0x20000000            // MAX_UNPACKED, 512 MiB
    cmp     x8, x10
    b.hi    L(fail)
    b       L(litlen_byte)
L(litlen_done):
    add     x15, x15, x8

L(slow_lit_ready):
    add     x16, x23, x15               // one past the literals
    cmp     x16, x20
    b.hi    L(fail)                      // the run runs off the input
    add     x17, x24, x15
    cmp     x17, x22
    b.hi    L(fail)                      // more output than was declared

    cbz     x15, L(slow_lit_done)
    add     x0, x21, x24
    add     x1, x19, x23

    /*
     * One vector move for a short run, before considering the block loop.
     *
     * Counting the blocks a real value decomposes into says why: on data that
     * compresses six-fold the average literal run is 1.4 bytes, and every one
     * of them was going through a 32-byte block copy. That is 53 KiB of store
     * traffic to move 2 KiB of literals in a 64 KiB value.
     *
     * The destination has room for sixteen whatever the count -- the run was
     * already checked against the declared length and the buffer carries 64
     * bytes past it. The source is the only question, and only within sixteen
     * bytes of the end of the stream.
     */
    cmp     x15, #16
    b.hi    L(slow_lit_wide)
    add     x9, x23, #16
    cmp     x9, x20
    b.hi    L(slow_lit_narrow)
    ldr     q0, [x1]
    str     q0, [x0]
    b       L(slow_lit_done)
L(slow_lit_narrow):
    mov     x2, x15
    bl      L(exact)
    b       L(slow_lit_done)

    // Wide while a whole block is readable from the input, exact for the tail:
    // overrunning the input would read memory that is not ours. The output side
    // needs no test -- see the header.
L(slow_lit_wide):
    add     x9, x16, #32
    cmp     x9, x20
    mov     x2, x15
    b.hi    L(slow_lit_exact)
    mov     x5, #0
L(litcp_block):
    ldp     q0, q1, [x1]
    stp     q0, q1, [x0]
    add     x5, x5, #32
    cmp     x5, x2
    b.hs    L(litcp_out)
    add     x1, x1, #32
    add     x0, x0, #32
    b       L(litcp_block)
L(litcp_out):
    b       L(slow_lit_done)
L(slow_lit_exact):
#ifdef KEVA_LZ4
    /*
     * The last literal run, in blocks that carry their own addressing.
     *
     * This is where an incompressible value spends all of its time: liblz4
     * writes one literal run covering the whole block, that run ends exactly at
     * the end of the input, so the test above sends it here -- through a call,
     * and then through a loop that spends four of its eight instructions moving
     * two pointers and comparing a counter.
     *
     * AArch64 does not need any of the four. `ldp`/`stp` post-index advance the
     * pointer they read, and `subs` sets the flag the branch wants. Eight
     * instructions a block become four, and the `bl`/`ret` around them goes as
     * well.
     *
     * The x86 body's version of this had to fix a width problem first -- there
     * the tail path moved sixteen bytes an iteration where the block loop moved
     * thirty-two. Here both already move thirty-two, so only the bookkeeping is
     * left, and that is the whole of the difference.
     *
     * Under KEVA_LZ4 only: our own format's last run is short, because the
     * value in front of it compressed, and the promise attached to that body is
     * that it does not change.
     */
    cmp     x2, #32
    b.lo    L(lz4_lit_tail)
L(lz4_lit_blk):
    ldp     q0, q1, [x1], #32
    stp     q0, q1, [x0], #32
    subs    x2, x2, #32
    cmp     x2, #32
    b.hs    L(lz4_lit_blk)
L(lz4_lit_tail):
    cbz     x2, L(slow_lit_done)
    bl      L(exact)
    b       L(slow_lit_done)
#endif
    bl      L(exact)

L(slow_lit_done):
    mov     x24, x17
    mov     x23, x16

    cmp     x24, x22
    b.hs    L(done)                      // the value ends on a literal run

    add     x9, x23, #2
    cmp     x9, x20
    b.hi    L(fail)
    ldrh    w11, [x19, x23]             // offset, kept clear of the length reader
    mov     x23, x9

    mov     x14, x26                    // match length, less MIN_MATCH
    cmp     x26, #MAT_MAX
    b.ne    L(slow_mat_ready)
    /*
     * The extended length, read here rather than called.
     *
     * On data that compresses well this runs once per block, and a call for it
     * is a branch out, a branch back and a return-stack slot to move at most a
     * couple of bytes. liblz4 has it as a self-loop inside its decode loop for
     * the same reason.
     */
    mov     x8, #0
L(matlen_byte):
    cmp     x23, x20
    b.hs    L(fail)
    ldrb    w9, [x19, x23]
    add     x23, x23, #1
    add     x8, x8, x9
    cmp     w9, #255
    b.ne    L(matlen_done)
    // A run of 255s on a corrupt stream would otherwise spin until it wrapped.
    mov     x10, #0x20000000            // MAX_UNPACKED, 512 MiB
    cmp     x8, x10
    b.hi    L(fail)
    b       L(matlen_byte)
L(matlen_done):
    add     x14, x14, x8
L(slow_mat_ready):
    add     x14, x14, #4

    cbz     w11, L(fail)
    cmp     x11, x24
    b.hi    L(fail)
    add     x12, x24, x14
    cmp     x12, x22
    b.hi    L(fail)

    add     x13, x21, x24
    sub     x15, x13, x11
    cmp     x11, #32
    b.lo    L(slow_near)

    mov     x2, x14
    mov     x5, #0
L(matcp_block):
    ldp     q0, q1, [x15]
    stp     q0, q1, [x13]
    add     x5, x5, #32
    cmp     x5, x2
    b.hs    L(matcp_out)
    add     x15, x15, #32
    add     x13, x13, #32
    b       L(matcp_block)
L(matcp_out):
    mov     x24, x12
    b       L(fast)

L(slow_near):
    mov     x0, x13
    mov     x1, x15
    mov     x2, x11
    mov     x3, x14
    bl      L(repeat)
    mov     x24, x12
    b       L(fast)

L(done):
    cmp     x24, x22
    b.ne    L(fail)
    mov     w0, w22
    b       L(return)

L(fail):
    mov     w0, wzr
L(return):
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
L(exact):
    mov     x6, x2
L(ex_32):
    cmp     x6, #32
    b.lo    L(ex_16)
    ldp     q0, q1, [x1]
    stp     q0, q1, [x0]
    add     x1, x1, #32
    add     x0, x0, #32
    sub     x6, x6, #32
    b       L(ex_32)
L(ex_16):
    tbz     x6, #4, L(ex_8)
    ldr     q0, [x1], #16
    str     q0, [x0], #16
L(ex_8):
    tbz     x6, #3, L(ex_4)
    ldr     x4, [x1], #8
    str     x4, [x0], #8
L(ex_4):
    tbz     x6, #2, L(ex_2)
    ldr     w4, [x1], #4
    str     w4, [x0], #4
L(ex_2):
    tbz     x6, #1, L(ex_1)
    ldrh    w4, [x1], #2
    strh    w4, [x0], #2
L(ex_1):
    tbz     x6, #0, L(ex_ret)
    ldrb    w4, [x1]
    strb    w4, [x0]
L(ex_ret):
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
 * Not a leaf: it calls L(exact), so x30 is saved. Clobbers x0-x7, v0, v1, and
 * relies on L(exact) leaving x3, x5 and x7 alone.
 */
L(repeat):
#ifdef KEVA_LZ4
    /*
     * A repeat shorter than a vector, built in one shuffle.
     *
     * Below this, the only wide case was an offset of exactly one; everything
     * from two to fifteen went down the doubling ladder at L(rep_double),
     * which calls L(exact) once per doubling step and reloads its pointers
     * from the stack each time. A twenty-byte match at offset four is three
     * calls. Repetitive data is made of exactly those -- JSON with the same
     * field names over and over is short offsets and moderate lengths -- so
     * that ladder is most of what records_512 spends its time in.
     *
     * The x86 body has not done this since it was written: it loads the live
     * bytes, turns them into a full window with one `pshufb`, and then splats
     * sixteen at a time, advancing the window with a second shuffle. NEON's
     * `tbl` has the same byte-index semantics -- index j takes source byte j,
     * anything past fifteen gives zero -- so the two tables carry over
     * unchanged from the x86 source.
     *
     * `adr` rather than `adrp`: a megabyte of reach is plenty for a table in
     * the same file, and it is spelled the same on Mach-O and ELF, where the
     * page-relative forms are not.
     *
     * The sixteen-byte load reads past the live bytes and the store past the
     * length. Both are already how this path works -- the offset-one case
     * below stores in whole vectors too -- and the caller's buffer carries
     * sixty-four bytes of slack for it.
     */
    cmp     x2, #16
    b.hs    L(rep_checked)
    ldr     q0, [x1]                    // the live bytes, and some noise
    lsl     x9, x2, #4                  // offset -> table index
    adr     x10, L(splat_first)
    ldr     q2, [x10, x9]
    adr     x10, L(splat_next)
    ldr     q3, [x10, x9]
    tbl     v0.16b, {v0.16b}, v2.16b    // j -> j mod offset
    mov     x5, #0
L(lz4_splat):
    str     q0, [x0, x5]
    add     x5, x5, #16
    tbl     v0.16b, {v0.16b}, v3.16b    // advance the window by sixteen
    cmp     x5, x3
    b.lo    L(lz4_splat)
    ret
L(rep_checked):
#endif
    stp     x29, x30, [sp, #-32]!
    mov     x29, sp
    stp     x0, x1, [sp, #16]           // destination base and source

    cmp     x2, #1
    b.ne    L(rep_double)

    ld1r    {v0.16b}, [x1]
    mov     x5, #0
L(rep_splat):
    str     q0, [x0, x5]
    add     x5, x5, #16
    cmp     x5, x3
    b.lo    L(rep_splat)
    b       L(rep_ret)

L(rep_double):
    mov     x7, x2                      // this step's width
    mov     x5, #0                      // written so far
L(rep_step):
    sub     x6, x3, x5                  // still owed
    cmp     x7, x6
    csel    x6, x7, x6, lo              // take = min(width, owed)
    ldp     x0, x1, [sp, #16]
    add     x0, x0, x5
    mov     x2, x6
    // Advanced before the call, because L(exact) is free to destroy x2 and x6.
    add     x5, x5, x6
    lsl     x7, x7, #1
    bl      L(exact)
    cmp     x5, x3
    b.lo    L(rep_step)

L(rep_ret):


#ifdef KEVA_LZ4
/*
 * Is this block one literal run, exactly filling the output?
 *
 * Two exact equalities: it ends precisely at the end of the input, and it
 * produces precisely the declared length. Anything else goes back to the loops,
 * which are unchanged.
 *
 * The chain needs no bound of its own -- each turn advances the cursor by one
 * and adds at most 255, so the input runs out long before the length matters,
 * and the equalities reject whatever it reached.
 *
 * It can refuse to fire; it cannot fire wrongly. Every way of getting it wrong
 * sends the block back to the loops, which is why no mutation of it fails a
 * test and why only a measurement shows that it fires at all.
 */
L(lit_only):
    lsr     w5, w4, #LIT_SHIFT
    add     x6, x23, #1                 // past the token
    cmp     w5, #LIT_MAX
    b.ne    L(lit_only_ends)
    mov     x5, #0
L(lit_only_chain):
    cmp     x6, x20
    b.hs    L(lit_only_back)
    ldrb    w7, [x19, x6]
    add     x6, x6, #1
    add     x5, x5, x7
    cmp     w7, #255
    b.eq    L(lit_only_chain)
    add     x5, x5, #LIT_MAX
L(lit_only_ends):
    add     x9, x6, x5                  // where the run ends in the input
    cmp     x9, x20
    b.ne    L(lit_only_back)
    add     x17, x24, x5                // and in the output
    cmp     x17, x22
    b.ne    L(lit_only_back)
    cbz     x5, L(lit_only_back)
    mov     x15, x5
    mov     x16, x9
    add     x0, x21, x24
    add     x1, x19, x6
    mov     x2, x5
    b       L(slow_lit_exact)
#endif

#ifdef KEVA_LZ4
/*
 * Shuffle masks for a repeat closer than a vector, indexed by offset * 16.
 *
 * `splat_first` turns d live bytes into a full sixteen-byte window: j -> j mod
 * d. `splat_next` advances a window by sixteen: j -> (j + 16) mod d, which is
 * the same window shifted, so applying it repeatedly walks the pattern. Row
 * zero is never used; an offset of zero is rejected long before this.
 *
 * Byte-for-byte the tables in the x86 body, because `tbl` and `pshufb` read
 * their indices the same way.
 */
    .p2align 4
L(splat_first):
    .byte     0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0    // d = 0
    .byte     0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0    // d = 1
    .byte     0,  1,  0,  1,  0,  1,  0,  1,  0,  1,  0,  1,  0,  1,  0,  1    // d = 2
    .byte     0,  1,  2,  0,  1,  2,  0,  1,  2,  0,  1,  2,  0,  1,  2,  0    // d = 3
    .byte     0,  1,  2,  3,  0,  1,  2,  3,  0,  1,  2,  3,  0,  1,  2,  3    // d = 4
    .byte     0,  1,  2,  3,  4,  0,  1,  2,  3,  4,  0,  1,  2,  3,  4,  0    // d = 5
    .byte     0,  1,  2,  3,  4,  5,  0,  1,  2,  3,  4,  5,  0,  1,  2,  3    // d = 6
    .byte     0,  1,  2,  3,  4,  5,  6,  0,  1,  2,  3,  4,  5,  6,  0,  1    // d = 7
    .byte     0,  1,  2,  3,  4,  5,  6,  7,  0,  1,  2,  3,  4,  5,  6,  7    // d = 8
    .byte     0,  1,  2,  3,  4,  5,  6,  7,  8,  0,  1,  2,  3,  4,  5,  6    // d = 9
    .byte     0,  1,  2,  3,  4,  5,  6,  7,  8,  9,  0,  1,  2,  3,  4,  5    // d = 10
    .byte     0,  1,  2,  3,  4,  5,  6,  7,  8,  9, 10,  0,  1,  2,  3,  4    // d = 11
    .byte     0,  1,  2,  3,  4,  5,  6,  7,  8,  9, 10, 11,  0,  1,  2,  3    // d = 12
    .byte     0,  1,  2,  3,  4,  5,  6,  7,  8,  9, 10, 11, 12,  0,  1,  2    // d = 13
    .byte     0,  1,  2,  3,  4,  5,  6,  7,  8,  9, 10, 11, 12, 13,  0,  1    // d = 14
    .byte     0,  1,  2,  3,  4,  5,  6,  7,  8,  9, 10, 11, 12, 13, 14,  0    // d = 15

    .p2align 4
L(splat_next):
    .byte     0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0    // d = 0
    .byte     0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0    // d = 1
    .byte     0,  1,  0,  1,  0,  1,  0,  1,  0,  1,  0,  1,  0,  1,  0,  1    // d = 2
    .byte     1,  2,  0,  1,  2,  0,  1,  2,  0,  1,  2,  0,  1,  2,  0,  1    // d = 3
    .byte     0,  1,  2,  3,  0,  1,  2,  3,  0,  1,  2,  3,  0,  1,  2,  3    // d = 4
    .byte     1,  2,  3,  4,  0,  1,  2,  3,  4,  0,  1,  2,  3,  4,  0,  1    // d = 5
    .byte     4,  5,  0,  1,  2,  3,  4,  5,  0,  1,  2,  3,  4,  5,  0,  1    // d = 6
    .byte     2,  3,  4,  5,  6,  0,  1,  2,  3,  4,  5,  6,  0,  1,  2,  3    // d = 7
    .byte     0,  1,  2,  3,  4,  5,  6,  7,  0,  1,  2,  3,  4,  5,  6,  7    // d = 8
    .byte     7,  8,  0,  1,  2,  3,  4,  5,  6,  7,  8,  0,  1,  2,  3,  4    // d = 9
    .byte     6,  7,  8,  9,  0,  1,  2,  3,  4,  5,  6,  7,  8,  9,  0,  1    // d = 10
    .byte     5,  6,  7,  8,  9, 10,  0,  1,  2,  3,  4,  5,  6,  7,  8,  9    // d = 11
    .byte     4,  5,  6,  7,  8,  9, 10, 11,  0,  1,  2,  3,  4,  5,  6,  7    // d = 12
    .byte     3,  4,  5,  6,  7,  8,  9, 10, 11, 12,  0,  1,  2,  3,  4,  5    // d = 13
    .byte     2,  3,  4,  5,  6,  7,  8,  9, 10, 11, 12, 13,  0,  1,  2,  3    // d = 14
    .byte     1,  2,  3,  4,  5,  6,  7,  8,  9, 10, 11, 12, 13, 14,  0,  1    // d = 15
#endif
    ldp     x29, x30, [sp], #32
    ret

#if !defined(__APPLE__)
    .size KEVA_NAME(KEVA_ENTRY), . - KEVA_NAME(KEVA_ENTRY)
    .section .note.GNU-stack, "", %progbits
#endif
