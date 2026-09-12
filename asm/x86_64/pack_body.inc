/*
 * One instantiation of the packer's pass. Included four times by pack.S.
 *
 * Parameters, every one of them an immediate by the time it reaches an
 * instruction:
 *
 *   PACK_PFX        label prefix, unique per instantiation
 *   PACK_HSHIFT     52 (twelve table bits) or 53 (eleven, above 8 KiB)
 *   PACK_LIT_MAX    15 for the even split, 3 for the wide one
 *   PACK_MAT_MAX    15 or 63
 *   PACK_TOK_SHIFT  how far up the token the literal field sits: 4 or 6
 *   PACK_ADAPTIVE   1 in an even body, 0 in a wide one
 *   PACK_SWITCH_TO  the wide body's emit entry, for when the split changes
 *
 * # Why four copies rather than four registers
 *
 * The Rust packer makes the same choice for the same reason (pack.rs:734-745):
 * x86 has no register-to-register variable shift outside BMI2, so a shift held
 * in a value has to sit in CL -- and CL is wanted two instructions later for
 * the candidate, so the generated code reloaded it on every position visited.
 * As an immediate it is `shrq $52` and CL stays free. The split's three fields
 * go the same way, which is also what the decoder does with its two.
 *
 * # The register map, and why it is identical in all four
 *
 *   r15 src        r14 table      r13 at         r12 next_at
 *   rbx misses     rbp out-p      r11 last       r10 literal_start
 *   rsi hash mult
 *   rax rcx rdx rdi r9 r8   temps
 *
 * Fixed across the four so the even body can jump straight into the wide one
 * when the split changes: nothing is marshalled at the boundary, the two
 * bodies simply continue with the same state in the same places.
 *
 * # What is on the stack, and why that is not a violation of register-only
 *
 * `recent`, `seen`, the switch positions and the header length live in the
 * frame. Every one of them is touched once per *block* -- on the emit path,
 * which a value enters once per match -- and never once per position. The
 * search loop below does not read or write memory except the input and the
 * table, which are the data. Holding a per-block counter in a register would
 * cost the match path a temp it needs, and the match path is where the temps
 * are actually scarce.
 *
 * `misses` is reset after the block is written rather than before it, which is
 * equivalent -- nothing between the two reads it -- and buys the emit path a
 * fifth temp. `next_at` is dead from the moment a match is confirmed until it
 * is reassigned, so it is a temp there too.
 */

#define PLBL(p, n) KEVA_LABEL(p, n)
#define L_(n) PLBL(PACK_PFX, n)

/*
 * The search. One position per iteration, and almost every one is rejected, so
 * what matters here is the order the questions are asked in.
 */
    /*
     * The loop's exits, placed above it so the branches that take them fit in
     * one byte of displacement.
     *
     * Every exit targets code that sits past the block writer, several hundred
     * bytes away, and a conditional branch that far is `rel32` -- six bytes
     * against two. Four of them are in the loop. Routing each through a relay
     * that is a backward jump of under a hundred bytes makes all four `rel8`
     * and takes sixteen bytes out of the body, which is sixteen bytes of
     * instruction fetch on every position visited.
     *
     * Above the loop rather than below it, so the fall-through into the match
     * path stays a fall-through: relays after the body would need a jump over
     * them, and that jump would be taken on every match.
     */
    /*
     * The stride-one lane.
     *
     * The general loop below recomputes `misses >> SKIP_TRIGGER` at every
     * position, and on anything that compresses the answer is always one: a
     * match resets the counter to its trigger, and data that compresses finds
     * one long before thirty-two misses accumulate. Profiling `varied` put the
     * advance and the miss bookkeeping at 15.7% of all cycles -- more than the
     * whole hash -- for arithmetic whose result never changes.
     *
     * So the common case gets its own lane: the cursor is incremented, the
     * counter counts down instead of up, and the bound test folds into the
     * branch that closes the loop. Seventeen instructions per rejected position
     * become fourteen. On `varied` that is cycles one for one, because the
     * search runs at the instruction limit -- 18.09 per byte at IPC 5.8 is 3.12
     * cycles, and 3.14 was measured.
     *
     * Not a special case: stride one is where every compressible value spends
     * nearly all of its positions. The general loop keeps the accelerating
     * case, which is what incompressible data walks into after thirty-two
     * consecutive misses.
     */
L_(fast_entry):
    cmp     %r11, %r13
    ja      L_(tail)
    /*
     * One limit instead of two tests. The lane used to ask twice per position:
     * has the counter run out, and is the cursor past the end. Both are "may
     * this position be visited", so both fold into a single bound.
     *
     * The bound stops one short of `last`, because the lane reads the position
     * ahead of the one it is judging -- see below -- and at `last` that read
     * would go a byte past the input. Whatever the lane leaves goes to the
     * general loop, which reads only the position it is on. Which positions
     * get visited does not change, only which loop visits them.
     */
    mov     %r11, %rbx
    sub     $1, %rbx                        /* last - 1 */
    jb      L_(fast_handover)               /* last was zero */
    cmp     %r13, %rbx
    jb      L_(fast_handover)               /* only `last` is left */
    lea     31(%r13), %rcx
    cmp     %rcx, %rbx
    cmova   %rcx, %rbx                      /* min(last - 1, at + 31) */

    /*
     * The first position's hash, so the loop can be entered with one already
     * in hand. Everything inside it is then one position ahead of what it is
     * judging.
     */
    mov     (%r15,%r13), %rax               /* word8(at) */
    mov     %rax, %rdx
    imul    %rsi, %rdx                      /* the <<24 is folded into rsi */
    shr     $PACK_HSHIFT, %rdx              /* slot(at) */

    .p2align 5
L_(fast):
    /*
     * Two positions in flight.
     *
     * The chain from a position's bytes to its verdict is a load, a multiply,
     * a shift and a second load -- around seventeen cycles, and every one of
     * them was previously spent before the next position's chain could start.
     * The loop measured IPC 4.96 against a machine about six wide: not short
     * of work to issue, short of work that was ready.
     *
     * Nothing in position N+1's hash depends on N's verdict, so it is started
     * first and finished while N is being judged. The two chains overlap
     * instead of queueing, and the loop carries the result across the back
     * edge in `rax` and `rdx`.
     *
     * This is liblz4's prologue idea -- it issues two multiplies before
     * entering its loop for the same reason.
     */
    mov     1(%r15,%r13), %r8               /* word8(at + 1) */
    mov     %r8, %r9
    imul    %rsi, %r9
    shr     $PACK_HSHIFT, %r9               /* slot(at + 1), independent */

    mov     (%r14,%rdx,4), %ecx             /* stored, for `at` */
    mov     %r13d, (%r14,%rdx,4)            /* table[slot] = at */
    cmp     %r13, %rcx
    jae     L_(fast_miss)
    mov     %r13d, %edi
    sub     %rcx, %rdi
    cmp     $65535, %edi                    /* MAX_OFFSET */
    ja      L_(fast_miss)
    cmp     (%r15,%rcx), %eax               /* word4(candidate) == here */
    je      L_(to_match)

L_(fast_miss):
    inc     %r13
    mov     %r8, %rax                       /* the position ahead becomes this */
    mov     %r9, %rdx
    cmp     %rbx, %r13
    jbe     L_(fast)

    /*
     * Past the limit: the input ran out, thirty-two misses did, or the lane
     * reached the position it may not read ahead of. Asked here rather than in
     * the loop, once per lane rather than once per position. The general loop
     * takes over with `at` already standing where it reads `next_at`.
     *
     * The counter it is handed is the trigger doubled. Where the lane stopped
     * for the other two reasons only `last` itself remains, and a stride over
     * one position is not observable.
     */
L_(fast_handover):
    cmp     %r11, %r13
    ja      L_(tail)
    mov     %r13, %r12
    mov     $64, %rbx
    jmp     L_(scan)

/* The lane's match exit, relayed. Its target sits 132 bytes on, five past
 * what rel8 reaches, so the branch assembled six bytes wide -- and a branch is
 * fetched on every iteration whether or not it is taken. From here it is two.
 * Placed behind the unconditional jump above so nothing falls into it. */
L_(to_match):
    jmp     L_(match)

/* The general loop's exits, moved down to sit against it. They used to lead
 * the body, which was right until the stride-one lane was inserted between
 * them and their loop and pushed every one of them past rel8's reach. */
L_(miss):
    inc     %rbx
    jmp     L_(scan)
L_(to_tail):
    jmp     L_(tail)

    .p2align 5
L_(scan):
    /* at = next_at; next_at = at + (misses >> SKIP_TRIGGER)
     *
     * The advance sits before the probe rather than after it, so the address
     * this iteration loads from was formed during the last one and does not
     * wait on the branch that closed it. */
    mov     %r12, %r13
    /* The stride in 32 bits, which is two bytes shorter than the 64-bit form.
     * `misses` counts positions visited and a value cannot reach two gigabytes,
     * so it fits -- and the shift zero-extends, so the lea still sees the whole
     * register. Those two bytes are what keeps the window test below out of a
     * 32-byte boundary; see the note there. */
    mov     %ebx, %eax
    shr     $5, %eax                        /* SKIP_TRIGGER */
    lea     (%r13,%rax), %r12
    cmp     %r11, %r13
    ja      L_(to_tail)

    /* One load, not two. The four bytes a candidate is rejected on are the low
     * half of the eight the hash reads, at the same address, so taking them
     * from the register is exact and saves a load per position. */
    mov     (%r15,%r13), %rax               /* word8(at) */
    mov     %rax, %rdx
    imul    %rsi, %rdx                      /* hash5; the <<24 is folded into rsi */
    shr     $PACK_HSHIFT, %rdx

    mov     (%r14,%rdx,4), %ecx             /* stored */
    mov     %r13d, (%r14,%rdx,4)            /* table[slot] = at */

    /* Three questions, one of which the sentinel already answered. EMPTY is
     * 0x80000000, far above any reachable position, so "this slot was never
     * written" fails the same unsigned compare as "this candidate is not
     * behind the cursor". There is no empty test in this loop. */
    cmp     %r13, %rcx
    jae     L_(miss)
    /*
     * MAX_OFFSET, computed and compared in 32 bits. The candidate is already
     * known to be behind the cursor and a position cannot reach two gigabytes,
     * so the distance fits, and rdx is overwritten with the candidate a few
     * instructions later so its upper half is not wanted.
     *
     * The mixed widths here are chosen for encoded length, and the length is
     * load-bearing rather than tidy. Both this `cmp`+`ja` and the `cmp`+`jne`
     * below it are macro-fused pairs, and a fused pair that crosses a 32-byte
     * boundary is not held in the uop cache on the Skylake family -- which is
     * every Xeon up to Ice Lake. With `sub` in 32 bits this pair ended one byte
     * clear of a boundary and pushed the next pair across it instead; in 64
     * bits the pair ends exactly on the boundary and the next one starts on it,
     * and all four of the loop's pairs are clean.
     *
     * The loop head is `.p2align 5` so this is a property of the code rather
     * than of where the linker happened to put the function. It is still
     * fragile: anything added to the loop above shifts it. The disassembly is
     * the only thing that says whether it still holds.
     */
    mov     %r13d, %edx
    sub     %rcx, %rdx
    cmp     $65535, %edx
    ja      L_(miss)
    cmp     (%r15,%rcx), %eax               /* word4(candidate) == here */
    jne     L_(miss)

L_(match):
    /*
     * A match, reached from either lane with the same three registers live:
     * rax the eight bytes at `at`, rcx the candidate, r13 the position.
     *
     * From here the position is committed, so next_at (r12) and
     * misses (rbx) are dead and serve as temps until they are reassigned.
     *
     * rdx holds the candidate and rax the match length for the rest of this
     * block; everything else is scratch.
     */
    mov     %rcx, %rdx                      /* candidate */

    /* common_prefix(candidate + 4, at + 4): eight bytes at a time, and the
     * first differing byte is the lowest set bit of the xor because the loads
     * are little-endian. */
    /*
     * Shaped after liblz4's, which was read rather than guessed at. The first
     * form recomputed the bound at the top of every iteration with an lea and
     * a compare, then closed with an unconditional jmp -- eight instructions,
     * two of them address arithmetic the loads had to wait behind, and one
     * doing no work at all.
     *
     * Here the bound is a loop-invariant address formed once, the cursors are
     * pointers rather than an index, and the test sits at the bottom where it
     * fuses with the branch that closes the loop. Seven instructions, the
     * loads issue on entry, and nothing is recomputed.
     */
    lea     4(%r15,%rdx), %rdi              /* a */
    lea     4(%r15,%r13), %r9               /* b */
    mov     %r9, %rbx                       /* where the run starts */
    mov     PACK_SRC_END(%rsp), %r8
    sub     $8, %r8                         /* last address a whole word starts at */
    cmp     %r8, %r9
    ja      2f
1:  mov     (%rdi), %rcx
    xor     (%r9), %rcx
    jnz     3f
    add     $8, %rdi
    add     $8, %r9
    cmp     %r8, %r9
    jbe     1b
    jmp     2f
3:  bsf     %rcx, %rcx                      /* little-endian: the lowest set bit */
    shr     $3, %rcx
    add     %rcx, %r9
    jmp     4f
2:  add     $8, %r8                         /* the end itself, for the tail */
5:  cmp     %r8, %r9
    jae     4f
    mov     (%rdi), %cl
    cmp     (%r9), %cl
    jne     4f
    inc     %rdi
    inc     %r9
    jmp     5b
4:  sub     %rbx, %r9
    lea     4(%r9), %rax                    /* matched = MIN_MATCH + run */

    /*
     * Lazy. The first match found is not always the one worth taking: a
     * four-byte match here hides a forty-byte one a byte later, and a longer
     * match is one block instead of two, which is what the decoder is paid by.
     *
     * Gated at LAZY_UNTIL because once the match is long the second search is
     * nearly always wasted -- on 64 KiB of `varied` it runs 4138 times and
     * wins 521.
     */
    /*
     * The gate, and the lazy block's exits relayed for the same reason the
     * search loop's are: `lazy_done` sits past the inlined length comparison,
     * 184 bytes away, so three of the branches leaving here assembled as
     * six-byte rel32. From a relay directly above the block each of them is a
     * two-byte backward jump.
     *
     * The gate is written as a jump *into* the block rather than out of it, so
     * the relay is stepped over instead of fallen into.
     */
    cmp     $16, %rax                       /* LAZY_UNTIL */
    jb      L_(lazy)
    jmp     L_(lazy_done)
L_(to_lazydone):
    jmp     L_(lazy_done)

L_(lazy):
    lea     1(%r13), %rcx                   /* next = at + LAZY_REACH */
    cmp     %r11, %rcx
    ja      L_(to_lazydone)

    mov     (%r15,%rcx), %r12               /* word8(next) */
    mov     %r12, %rbx
    imul    %rsi, %rbx
    shr     $PACK_HSHIFT, %rbx

    mov     (%r14,%rbx,4), %edi             /* other */
    mov     %ecx, (%r14,%rbx,4)             /* inserted whether or not it wins */

    cmp     %rcx, %rdi
    jae     L_(to_lazydone)
    mov     %rcx, %rbx
    sub     %rdi, %rbx
    cmp     $65535, %rbx
    ja      L_(to_lazydone)
    cmp     (%r15,%rdi), %r12d              /* word4(other) == word4(next) */
    jne     L_(to_lazydone)

    /* Its length, measured the same way. rax and rdx must survive, so this
     * runs on rbx, r8, r9 and r12 with the count in r9. */
    lea     4(%r15,%rdi), %rbx              /* a */
    lea     4(%r15,%rcx), %r12              /* b */
    mov     PACK_SRC_END(%rsp), %r8
    xor     %r9d, %r9d
1:  lea     8(%r12,%r9), %rdi
    cmp     %r8, %rdi
    ja      2f
    mov     (%rbx,%r9), %rdi
    xor     (%r12,%r9), %rdi
    jnz     3f
    add     $8, %r9
    jmp     1b
3:  bsf     %rdi, %rdi
    shr     $3, %rdi
    add     %rdi, %r9
    jmp     4f
2:  lea     (%r12,%r9), %rdi
    cmp     %r8, %rdi
    jae     4f
    mov     (%rbx,%r9), %dil
    cmp     (%r12,%r9), %dil
    jne     4f
    inc     %r9
    jmp     2b
4:  add     $4, %r9                         /* len */

    /* Strictly longer, and by enough to pay for the literals the step turns
     * loose. With LAZY_REACH at one, `len > matched + step - 1` is `len >
     * matched`. */
    /* Straight to the target, not through the relay above it. This exit sits
     * past the inlined length comparison and the relay is 155 bytes behind it,
     * which is outside rel8; `lazy_done` is a few bytes ahead and inside it.
     * Relays are for the exits at the top of the block, not this one. */
    cmp     %rax, %r9
    jbe     L_(lazy_done)
    mov     %r9, %rax                       /* matched = len */
    /* candidate = other. The length loop needed every temp there was and
     * consumed the register `other` arrived in, so it is recovered from the
     * pointer that loop was already carrying: rbx is `src + other + 4`. */
    lea     -4(%rbx), %rdx
    sub     %r15, %rdx
    mov     %rcx, %r13                      /* at = next */
    /* Unconditionally, and that is not an oversight: the LAZY_UNTIL gate is
     * asked once, before the loop is entered, and `continue 'lazy` in the Rust
     * jumps past it. A step that wins keeps stepping however long the match has
     * become, which terminates because every round demands a strictly longer
     * match than the last. Re-asking the gate here stopped the walk early and
     * moved offsets by one wherever it did. */
    jmp     L_(lazy)

L_(lazy_done):

    /*
     * Walk the match backwards into the literals that were about to be
     * emitted. Those bytes are already known to be equal -- the search only
     * ever moves forward, so it never looked at them.
     *
     * Worth more than the ratio it adds: a byte moved out of a literal run and
     * into a match is one the decoder copies in a block rather than one at a
     * time, and it shortens the run instead of splitting it, so the value ends
     * up with fewer and longer blocks.
     */
    xor     %r12d, %r12d                    /* back = 0 */
L_(back):
    cmp     %r12, %rdx                      /* candidate > back */
    jbe     L_(back_done)
    mov     %r13, %rcx
    sub     %r12, %rcx                      /* at - back */
    cmp     %r10, %rcx                      /* > literal_start */
    jbe     L_(back_done)
    mov     %rdx, %rdi
    sub     %r12, %rdi                      /* candidate - back */
    mov     -1(%r15,%rdi), %r9b
    cmp     -1(%r15,%rcx), %r9b
    jne     L_(back_done)
    inc     %r12
    jmp     L_(back)
L_(back_done):

    /* match_extra = matched + back - MIN_MATCH, which is the field that
     * overflows and therefore the one the split argues about. */
    lea     -4(%rax,%r12), %rcx

#if PACK_ADAPTIVE
    /*
     * The split decision, taken before this block is written so the block
     * already uses the split it argued for.
     *
     * A running window rather than a total: a value is not required to have
     * one character throughout, and a decision taken over the whole of it gets
     * a document with a blob in the middle wrong whichever way it goes.
     */
    /*
     * The window is carried as a count rather than recomputed from the bits.
     * `popcnt` would say it in one instruction and is not in the x86-64
     * baseline -- it arrives with SSE4.2 -- and asking for it would put a
     * feature test in front of a packer that otherwise needs none. Keeping the
     * count incrementally is cheaper than the instruction anyway: the bit
     * leaving the window is the one at the top of the mask, and the bit
     * entering is the block just judged.
     */
    mov     PACK_RECENT(%rsp), %rdi
    mov     %rdi, %r9
    shr     $15, %r9
    and     $1, %r9d                        /* the bit leaving the window */
    add     %rdi, %rdi                      /* recent <<= 1 */
    cmp     $PACK_MAT_MAX, %rcx
    jb      5f
    inc     %rdi                            /* | overflowed */
    dec     %r9                             /* entering, so net +1 */
5:  and     $0xFFFF, %edi
    mov     %rdi, PACK_RECENT(%rsp)
    mov     PACK_COUNT(%rsp), %rdi
    sub     %r9, %rdi
    mov     %rdi, PACK_COUNT(%rsp)
    mov     PACK_SEEN(%rsp), %r9
    inc     %r9
    mov     %r9, PACK_SEEN(%rsp)
    cmp     $16, %r9                        /* SPLIT_WINDOW */
    jb      6f
    cmp     $8, %rdi                        /* count * 2 > SPLIT_WINDOW */
    jbe     6f
    /* The trailer's two fields are fixed width, so a switch is only taken
     * where both positions can be represented. The window sits at the front of
     * the value, so this holds in practice; it is tested rather than argued
     * because the width cannot say more. */
    mov     %rbp, %r9
    sub     PACK_OUT_BASE(%rsp), %r9
    sub     PACK_HEADER_LEN(%rsp), %r9      /* cur.len() - header_len */
    cmp     PACK_SWITCH_CAP(%rsp), %r9
    jae     6f
    cmp     PACK_SWITCH_CAP(%rsp), %r10     /* literal_start */
    jae     6f
    mov     %r9, PACK_SWITCH_IN(%rsp)
    mov     %r10, PACK_SWITCH_OUT(%rsp)
    movq    $1, PACK_HAVE_SWITCH(%rsp)
    jmp     PACK_SWITCH_TO
6:
#endif

L_(emit):
    /*
     * The block. rax is the match length, rdx the candidate, r12 the backward
     * extension and rcx the match extra; everything else here is scratch.
     */
    mov     %r13, %rdi
    sub     %r12, %rdi                      /* lit_end = at - back */
    mov     %rdi, %r9
    sub     %r10, %r9                       /* lit_len */

    /* Whether the run can be copied at a fixed width. The run is a slice of
     * the input, so reading past it has to stay inside the input. */
    add     $16, %rdi                       /* WIDE_LITERAL */
    cmp     PACK_SRC_LEN(%rsp), %rdi
    ja      L_(to_emit_slow)
    cmp     $PACK_LIT_MAX, %r9
    jae     L_(to_emit_slow)
    cmp     $PACK_MAT_MAX, %rcx
    jae     L_(emit_fast1)

    /*
     * The block whose fields all fit: no extension chain on either length, and
     * a literal run far enough from the end of the input to be copied at a
     * fixed width. Counting says this is most blocks -- extensions appear on
     * about one in six.
     *
     * Written this way because the general path below is a dependency chain
     * rather than a sequence of stores. Each of its writes advances the cursor
     * and the next write's address waits on that advance: token, inc, chain,
     * inc, literals, add, offset, add -- eight to twelve dependent operations
     * per block, and the address generation for the next block cannot start
     * until the last of them retires. That is what holds the block writer at
     * IPC 4 while the compiler's version sustains 6, and it is worst on data
     * that compresses well, where blocks are many and short.
     *
     * Here the layout is known before anything is written: the token at zero,
     * the literals at one, the offset at one plus the run. So all three stores
     * take a displacement off the same base, none of them waits on another,
     * and the cursor moves once. This is the idiom liblz4 uses -- it advances
     * its cursor first and stores at a negative displacement, for the same
     * reason.
     */
    lea     (%r15,%r10), %rbx               /* the run's source */
    mov     %r9, %r8
    shl     $PACK_TOK_SHIFT, %r8
    or      %rcx, %r8
    mov     %r8b, (%rbp)                    /* token */
    movdqu  (%rbx), %xmm0
    movdqu  %xmm0, 1(%rbp)                  /* literals, sixteen written */
    mov     %r13, %r8
    sub     %rdx, %r8
    mov     %r8w, 1(%rbp,%r9)               /* offset, past the true length */
    lea     3(%rbp,%r9), %rbp               /* token + run + offset, once */
    jmp     L_(emit_done)

/* The emit dispatch's exit, relayed: its three tests sit eleven bytes further
 * from the general path than rel8 reaches, because the two fast blocks lie
 * between. Behind the unconditional jump above, so nothing falls in. */
L_(to_emit_slow):
    jmp     L_(emit_slow)

    /*
     * The same block with one extension byte on the match length.
     *
     * Peeled out because on data that compresses well it is not the exception
     * but the rule: pack.rs:1120 counts the match field overflowing in 99% of
     * blocks at a six-fold ratio, since matches there average far more than
     * the fifteen the nibble can name. The first version of this fast path
     * treated an extension as rare and sent it to the general path, which is
     * why restructuring the block writer moved nothing on `records` -- almost
     * no block was taking the fast path at all.
     *
     * One byte rather than a chain, because the chain is only longer when the
     * match exceeds 270 bytes. The decoder peeled the same case for the same
     * reason, in 43bf2d8.
     *
     * The layout is still known up front: token, run, offset, extension. Four
     * stores on displacements off one base, and the cursor moves once.
     */
L_(emit_fast1):
    mov     %rcx, %rdi
    sub     $PACK_MAT_MAX, %rdi
    cmp     $255, %rdi
    jae     L_(emit_slow)                   /* needs a chain, not one byte */

    lea     (%r15,%r10), %rbx
    mov     %r9, %r8
    shl     $PACK_TOK_SHIFT, %r8
    or      $PACK_MAT_MAX, %r8              /* the field is saturated */
    mov     %r8b, (%rbp)
    movdqu  (%rbx), %xmm0
    movdqu  %xmm0, 1(%rbp)
    mov     %r13, %r8
    sub     %rdx, %r8
    mov     %r8w, 1(%rbp,%r9)
    mov     %dil, 3(%rbp,%r9)               /* the extension byte */
    lea     4(%rbp,%r9), %rbp
    jmp     L_(emit_done)

L_(emit_slow):
    /* An extension chain, or a run too near the end of the input to overrun.
     * The wide test is asked again because the fast path consumed the answer. */
    mov     %r13, %rdi
    sub     %r12, %rdi
    add     $16, %rdi
    cmp     PACK_SRC_LEN(%rsp), %rdi
    setbe   %dil
    movzbl  %dil, %edi

    /* token = min(lit_len, LIT_MAX) << TOK_SHIFT | min(match_extra, MAT_MAX) */
    mov     $PACK_LIT_MAX, %r8
    cmp     %r8, %r9
    cmovb   %r9, %r8
    shl     $PACK_TOK_SHIFT, %r8
    mov     $PACK_MAT_MAX, %rbx
    cmp     %rbx, %rcx
    cmovb   %rcx, %rbx
    or      %rbx, %r8
    mov     %r8b, (%rbp)
    inc     %rbp

    /* An extended literal length: 255s until a byte that is not one. */
    cmp     $PACK_LIT_MAX, %r9
    jb      9f
    mov     %r9, %r8
    sub     $PACK_LIT_MAX, %r8
8:  cmp     $255, %r8
    jb      10f
    movb    $255, (%rbp)
    inc     %rbp
    sub     $255, %r8
    jmp     8b
10: mov     %r8b, (%rbp)
    inc     %rbp
9:

    /*
     * The literals, copied a block at a time and never a byte at a time.
     *
     * This is what liblz4 does with LZ4_wildCopy: write whole registers past
     * the end of the run and let the next block overwrite the surplus, rather
     * than walking a tail down 8, 4, 2, 1. The output can always take it --
     * `Cursor::room` reserves sixteen bytes and another sixty-four past the
     * worst case -- so the only thing that has to be bounded is the *source*,
     * which is the input and ends where it ends.
     *
     * `rdi` says a full block past the run is still inside the input. That
     * covers rounding the run up to whole blocks, since the rounding adds less
     * than a block. Only a run within sixteen bytes of the input's end fails
     * it, which is at most the last block of a value, and that one takes the
     * exact path below.
     *
     * The advance is the run's true length, so what the surplus wrote is not
     * part of the output.
     */
    lea     (%r15,%r10), %rbx               /* source */
    test    %rdi, %rdi
    jz      30f
    lea     (%rbp,%r9), %r8                 /* where the run ends */
11: movdqu  (%rbx), %xmm0
    movdqu  %xmm0, (%rbp)
    add     $16, %rbx
    add     $16, %rbp
    cmp     %r8, %rbp
    jb      11b
    mov     %r8, %rbp                       /* exact, whatever the surplus wrote */
    jmp     14f

    /*
     * Within a block of the input's end, so nothing may be read past the run
     * and nothing written past it either. Copied in overlapping pairs rather
     * than by walking a tail down 8, 4, 2, 1: for a run of five, four bytes
     * from the front and four from the back write three of them twice and the
     * whole run exactly once. Two independent copies per size class instead of
     * a chain of four tests, and no byte is ever moved on its own.
     *
     * Exact on both sides, which is the reason this path exists at all.
     */
30: cmp     $16, %r9
    jb      31f
    movdqu  (%rbx), %xmm0
    movdqu  %xmm0, (%rbp)
    add     $16, %rbx
    add     $16, %rbp
    sub     $16, %r9
    jmp     30b
31: cmp     $8, %r9
    jb      13f
    mov     (%rbx), %r8
    mov     -8(%rbx,%r9), %rdi
    mov     %r8, (%rbp)
    mov     %rdi, -8(%rbp,%r9)
    add     %r9, %rbp
    jmp     14f
13: cmp     $4, %r9
    jb      15f
    mov     (%rbx), %r8d
    mov     -4(%rbx,%r9), %edi
    mov     %r8d, (%rbp)
    mov     %edi, -4(%rbp,%r9)
    add     %r9, %rbp
    jmp     14f
15: cmp     $2, %r9
    jb      16f
    movzwl  (%rbx), %r8d
    movzwl  -2(%rbx,%r9), %edi
    mov     %r8w, (%rbp)
    mov     %di, -2(%rbp,%r9)
    add     %r9, %rbp
    jmp     14f
16: test    $1, %r9b
    jz      14f
    mov     (%rbx), %r8b
    mov     %r8b, (%rbp)
    inc     %rbp
14:

    /* The offset, always two bytes. */
    mov     %r13, %r8
    sub     %rdx, %r8
    mov     %r8w, (%rbp)
    add     $2, %rbp

    /* An extended match length, the same chain. */
    cmp     $PACK_MAT_MAX, %rcx
    jb      18f
    sub     $PACK_MAT_MAX, %rcx
17: cmp     $255, %rcx
    jb      19f
    movb    $255, (%rbp)
    inc     %rbp
    sub     $255, %rcx
    jmp     17b
19: mov     %cl, (%rbp)
    inc     %rbp
18:

L_(emit_done):
    /* at += matched; literal_start = at. A match resets the counter to its
     * trigger, which is stride one, so this returns to the fast lane rather
     * than to the general loop. `next_at` is not set: only the general loop
     * reads it, and the transition into it forms it. */
    add     %rax, %r13
    mov     %r13, %r10

    /*
     * No early bail-out, deliberately, though one would be sound: once the
     * output has reached the input length the answer is already 0 whatever
     * follows, since the output only grows.
     *
     * It is left out because it would not be *identical*. Stopping early
     * leaves the table with fewer insertions than the Rust packer's, and the
     * table is carried across values -- so the next value would be searched
     * against a different table and could pack differently. The byte-for-byte
     * test would then fail on a value that is not the one that diverged. The
     * caller sizes the output for the worst case instead, exactly as
     * `Cursor::room` does on the Rust side.
     */
    jmp     L_(fast_entry)

    /*
     * Everything left over is literals with no match to follow. A zero match
     * field and no offset behind it is what marks the tail; the decoder stops
     * because the declared length has been produced.
     */
L_(tail):
    mov     PACK_SRC_LEN(%rsp), %r9
    sub     %r10, %r9                       /* lit_len */
    mov     $PACK_LIT_MAX, %r8
    cmp     %r8, %r9
    cmovb   %r9, %r8
    shl     $PACK_TOK_SHIFT, %r8
    mov     %r8b, (%rbp)
    inc     %rbp

    cmp     $PACK_LIT_MAX, %r9
    jb      21f
    mov     %r9, %r8
    sub     $PACK_LIT_MAX, %r8
20: cmp     $255, %r8
    jb      22f
    movb    $255, (%rbp)
    inc     %rbp
    sub     $255, %r8
    jmp     20b
22: mov     %r8b, (%rbp)
    inc     %rbp
21:

    lea     (%r15,%r10), %rbx
23: cmp     $16, %r9
    jb      24f
    movdqu  (%rbx), %xmm0
    movdqu  %xmm0, (%rbp)
    add     $16, %rbx
    add     $16, %rbp
    sub     $16, %r9
    jmp     23b
24: test    $8, %r9b
    jz      25f
    mov     (%rbx), %r8
    mov     %r8, (%rbp)
    add     $8, %rbx
    add     $8, %rbp
25: test    $4, %r9b
    jz      26f
    mov     (%rbx), %r8d
    mov     %r8d, (%rbp)
    add     $4, %rbx
    add     $4, %rbp
26: test    $2, %r9b
    jz      27f
    movzwl  (%rbx), %r8d
    mov     %r8w, (%rbp)
    add     $2, %rbx
    add     $2, %rbp
27: test    $1, %r9b
    jz      28f
    mov     (%rbx), %r8b
    mov     %r8b, (%rbp)
    inc     %rbp
28:
    jmp     KEVA_LABEL(pack, finish)

#undef L_
#undef PLBL
