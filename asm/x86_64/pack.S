/*
 * keva_pack -- the whole packer, one call per value. x86-64.
 *
 *   uint32_t keva_pack(const uint8_t *src, uintptr_t src_len,
 *                      uint8_t *dst, uintptr_t dst_cap,
 *                      uint32_t *table);
 *
 * System V AMD64: rdi src, rsi src_len, rdx dst, rcx dst_cap, r8 table.
 * Result in eax: bytes written, or 0 when packing did not help and the caller
 * should store the original.
 *
 * # One call, the whole path
 *
 * The search was written by itself first and measured 4-17% ahead of the
 * compiler's version. Wired in at one call per match it made the packer 16-29%
 * *slower*: an `extern "C"` boundary is a handful of cycles, but it stops the
 * compiler carrying loop state in registers, and crossing it once per match
 * costs more than the kernel wins. The fix is not a faster boundary, it is
 * fewer of them -- so everything between a value arriving and its packed form
 * leaving happens here, with nothing crossing in between.
 *
 * That includes the two things the AArch64 kernel still leaves outside: the
 * probe that decides whether a value is worth searching at all, and the LEB128
 * header. Neither is measurable on its own -- both run once per value -- but
 * leaving them in Rust means a second implementation of a format that has to
 * agree exactly, and a path that re-runs the portable packer.
 *
 * # Output format, matching keva_core::store::pack
 *
 *   [header: varint][block][block]...[switch trailer, if hybrid]
 *
 *   header := varint(declared << 2 | hybrid << 1 | split)
 *   block  := [token][extended literal length][literals]
 *             [offset u16][extended match length]
 *
 * The token's eight bits are not fixed at four each: `split` chooses 4/4 or
 * 2/6, and `hybrid` says the stream changes from one to the other partway, with
 * the trailer giving the two positions where it does.
 *
 * # Four bodies
 *
 * The split's fields and the hash shift are immediates, not registers, so the
 * pass is assembled once per combination: two splits by two table widths. See
 * pack_body.inc for why, and for the register map they share -- it is shared
 * precisely so the even body can jump into the wide one mid-value.
 *
 * # Bounds
 *
 * There are no bounds checks in the block writer, and the reason is the same
 * one that makes the Rust packer sound: the caller reserves the worst case
 * before the first block, which is the whole input as literals plus a token and
 * an offset per block plus the chains of 255s plus the fixed-width literal
 * move's overshoot. That is `Cursor::room` on the Rust side, and it is checked
 * against `dst_cap` on entry here. A caller that passes less is refused.
 *
 * Constants are duplicated from pack.rs because a `.S` file cannot read Rust
 * consts. `the_duplicated_constants_still_hold` fails if they drift.
 *
 *   HASH_BITS 12 -> shift 52, or 11 -> 53 above 8192 bytes
 *   MAX_OFFSET 65535   MIN_MATCH 4   SKIP_TRIGGER 5   EMPTY 0x80000000
 *   LAZY_UNTIL 16   LAZY_REACH 1   SPLIT_WINDOW 16   WIDE_LITERAL 16
 *   PROBE 1024   PROBE_BITS 8   PROBE_WINDOWS 4   PROBE_ABOVE 2048
 */

#include "../common.h"

/*
 * The frame. Every slot here is written once per value or once per block, on
 * the emit path; the search loop never touches it. See pack_body.inc.
 */
#define PACK_RECENT         0
#define PACK_SEEN           8
#define PACK_COUNT          16
#define PACK_HEADER_LEN     24
#define PACK_SWITCH_IN      32
#define PACK_SWITCH_OUT     40
#define PACK_OUT_BASE       48
#define PACK_SRC_LEN        56
#define PACK_SRC_END        64
#define PACK_SWITCH_CAP     72
#define PACK_HAVE_SWITCH    80
#define PACK_PROBE          96
#define PACK_FRAME          1128

#define PACK_LBL(p, n) KEVA_LABEL(p, n)

    .text
    .p2align 4
    .globl KEVA_SYM(keva_pack)

KEVA_SYM(keva_pack):
    push    %rbx
    push    %rbp
    push    %r12
    push    %r13
    push    %r14
    push    %r15
    sub     $PACK_FRAME, %rsp

    /* Eight, not four: the hash reads eight bytes at every position it visits,
     * so a shorter value has nothing that can be searched. Nothing is lost --
     * a header, a token and an offset already outweigh a seven-byte value. */
    cmp     $8, %rsi
    jb      KEVA_LABEL(pack, give_up)

    /* dst_cap >= Cursor::room(src_len) + the longest header.
     *   room(n) = n + n/4*3 + n/128 + WIDE_LITERAL + 64  */
    mov     %rsi, %rax
    shr     $2, %rax
    lea     (%rax,%rax,2), %rax             /* n/4 * 3 */
    add     %rsi, %rax
    mov     %rsi, %r9
    shr     $7, %r9
    add     %r9, %rax
    add     $90, %rax                       /* 16 + 64 + varint room */
    cmp     %rax, %rcx
    jb      KEVA_LABEL(pack, give_up)

    mov     %rdi, %r15                      /* src */
    mov     %r8, %r14                       /* table */
    mov     %rdx, %rbp                      /* out cursor */
    mov     %rdx, PACK_OUT_BASE(%rsp)
    mov     %rsi, PACK_SRC_LEN(%rsp)
    lea     (%rdi,%rsi), %rax
    mov     %rax, PACK_SRC_END(%rsp)

    /*
     * Whether the value repeats itself inside its first kilobyte.
     *
     * A value that does not is not going to compress, and searching the rest of
     * it produces a stream longer than the input that is then thrown away --
     * 0.22 cycles per byte spent on 64 KiB of noise to answer a question a
     * kilobyte could answer.
     *
     * Four windows spread across the value rather than one at the front, so it
     * is judged on more than its head: a buffer written at a high offset or a
     * ciphertext followed by plaintext are not contrived shapes. Positions are
     * absolute and the table is shared between windows, so a word in the last
     * window still matches one recorded in the first.
     */
    cmp     $2048, %rsi                     /* PROBE_ABOVE */
    jbe     KEVA_LABEL(pack, worth)

    lea     PACK_PROBE(%rsp), %rdi
    xorps   %xmm0, %xmm0
    mov     $1024, %ecx
1:  movups  %xmm0, (%rdi)
    add     $16, %rdi
    sub     $16, %ecx
    jnz     1b

    mov     %rsi, %r8
    shr     $2, %r8                         /* step = len / PROBE_WINDOWS */
    lea     -4(%rsi), %r9                   /* len - MIN_MATCH */
    xor     %r10d, %r10d                    /* window index */

2:  mov     %r10, %r11
    imul    %r8, %r11                       /* start = i * step */
    lea     256(%r11), %rdx                 /* start + PROBE / PROBE_WINDOWS */
    cmp     %r9, %rdx
    cmovae  %r9, %rdx                       /* last = min(.., len - 4) */
    mov     %r11, %rax                      /* at */
    mov     $32, %rcx                       /* misses = 1 << SKIP_TRIGGER */

3:  cmp     %rdx, %rax
    ja      4f
    mov     (%r15,%rax), %edi               /* word4(at) */
    imul    $0x9E3779B1, %edi, %r11d
    shr     $24, %r11d                      /* 32 - PROBE_BITS */
    mov     PACK_PROBE(%rsp,%r11,4), %r12d  /* stored, one past the position */
    lea     1(%rax), %r13
    mov     %r13d, PACK_PROBE(%rsp,%r11,4)
    test    %r12d, %r12d
    jz      5f
    cmp     -1(%r15,%r12), %edi
    je      KEVA_LABEL(pack, worth)
5:  mov     %rcx, %r11
    shr     $5, %r11
    add     %r11, %rax
    inc     %rcx
    jmp     3b

4:  inc     %r10
    cmp     $4, %r10                        /* PROBE_WINDOWS */
    jb      2b
    jmp     KEVA_LABEL(pack, give_up)

KEVA_LABEL(pack, worth):
    /*
     * The header, written with the flag bits clear. Their value cannot be
     * known yet and does not need to be: the varint's length is fixed by the
     * declared size, so the pass commits to nothing and the bits are set at the
     * end if the split changed.
     */
    mov     PACK_SRC_LEN(%rsp), %rax
    shl     $2, %rax
6:  mov     %al, %cl
    and     $0x7F, %cl
    shr     $7, %rax
    jz      7f
    or      $0x80, %cl
    mov     %cl, (%rbp)
    inc     %rbp
    jmp     6b
7:  mov     %cl, (%rbp)
    inc     %rbp

    mov     %rbp, %rax
    sub     PACK_OUT_BASE(%rsp), %rax
    mov     %rax, PACK_HEADER_LEN(%rsp)

    /* The trailer's two fields are fixed width, and the width follows from the
     * declared length alone so the decoder knows it before it needs it. */
    mov     PACK_SRC_LEN(%rsp), %rax
    mov     $0x10000, %rcx
    cmp     %rcx, %rax
    ja      8f
    mov     %rcx, PACK_SWITCH_CAP(%rsp)     /* 1 << 16 */
    jmp     9f
8:  mov     $0x100000000, %rcx
    mov     %rcx, PACK_SWITCH_CAP(%rsp)     /* 1 << 32 */
9:

    mov     %rax, %rcx                      /* src_len, for the width test */
    xorps   %xmm0, %xmm0
    movups  %xmm0, PACK_RECENT(%rsp)        /* recent, seen */
    movq    $0, PACK_COUNT(%rsp)
    movq    $0, PACK_HAVE_SWITCH(%rsp)

    xor     %r13d, %r13d                    /* at */
    xor     %r12d, %r12d                    /* next_at */
    xor     %r10d, %r10d                    /* literal_start */
    mov     $32, %rbx                       /* misses = 1 << SKIP_TRIGGER */
    mov     PACK_SRC_LEN(%rsp), %r11
    sub     $8, %r11                        /* last = len - HASH_WORD */
    movabs  $889523592379, %rsi             /* the five-byte multiplier */

    /* Which of the two table widths this value gets. The whole table is always
     * allocated; the shift only decides how much of it a value touches, and
     * touched lines are what the cache charges for. */
    /* Entered at the stride-one lane: `misses` starts at its trigger, which
     * is a stride of one, so the general loop would compute that and nothing
     * else until thirty-two misses accumulate. */
    cmp     $8192, %rcx                     /* NARROW_TABLE_ABOVE */
    ja      PACK_LBL(el, fast_entry)
    jmp     PACK_LBL(es, fast_entry)

/* Even split, twelve table bits. The split may change from here. */
#define PACK_PFX            es
#define PACK_HSHIFT         52
#define PACK_LIT_MAX        15
#define PACK_MAT_MAX        15
#define PACK_TOK_SHIFT      4
#define PACK_ADAPTIVE       1
#define PACK_SWITCH_TO      PACK_LBL(ws, emit)
#include "pack_body.inc"
#undef PACK_PFX
#undef PACK_HSHIFT
#undef PACK_LIT_MAX
#undef PACK_MAT_MAX
#undef PACK_TOK_SHIFT
#undef PACK_ADAPTIVE
#undef PACK_SWITCH_TO

/* Even split, eleven table bits. */
#define PACK_PFX            el
#define PACK_HSHIFT         53
#define PACK_LIT_MAX        15
#define PACK_MAT_MAX        15
#define PACK_TOK_SHIFT      4
#define PACK_ADAPTIVE       1
#define PACK_SWITCH_TO      PACK_LBL(wl, emit)
#include "pack_body.inc"
#undef PACK_PFX
#undef PACK_HSHIFT
#undef PACK_LIT_MAX
#undef PACK_MAT_MAX
#undef PACK_TOK_SHIFT
#undef PACK_ADAPTIVE
#undef PACK_SWITCH_TO

/* Wide split, twelve table bits. Reached only from the even body. */
#define PACK_PFX            ws
#define PACK_HSHIFT         52
#define PACK_LIT_MAX        3
#define PACK_MAT_MAX        63
#define PACK_TOK_SHIFT      6
#define PACK_ADAPTIVE       0
#define PACK_SWITCH_TO      PACK_LBL(ws, emit)
#include "pack_body.inc"
#undef PACK_PFX
#undef PACK_HSHIFT
#undef PACK_LIT_MAX
#undef PACK_MAT_MAX
#undef PACK_TOK_SHIFT
#undef PACK_ADAPTIVE
#undef PACK_SWITCH_TO

/* Wide split, eleven table bits. */
#define PACK_PFX            wl
#define PACK_HSHIFT         53
#define PACK_LIT_MAX        3
#define PACK_MAT_MAX        63
#define PACK_TOK_SHIFT      6
#define PACK_ADAPTIVE       0
#define PACK_SWITCH_TO      PACK_LBL(wl, emit)
#include "pack_body.inc"
#undef PACK_PFX
#undef PACK_HSHIFT
#undef PACK_LIT_MAX
#undef PACK_MAT_MAX
#undef PACK_TOK_SHIFT
#undef PACK_ADAPTIVE
#undef PACK_SWITCH_TO

KEVA_LABEL(pack, finish):
    /*
     * The trailer, at the end rather than in the header so a value that never
     * switches pays nothing for the possibility. Fixed width because a varint
     * cannot be read backwards without ambiguity -- its continuation bits look
     * no different from a body byte's high bit.
     */
    cmpq    $0, PACK_HAVE_SWITCH(%rsp)
    je      10f
    mov     PACK_SWITCH_IN(%rsp), %rcx
    mov     PACK_SWITCH_OUT(%rsp), %rdx
    mov     $0x10000, %rax
    cmp     %rax, PACK_SRC_LEN(%rsp)
    ja      11f
    mov     %cx, (%rbp)
    mov     %dx, 2(%rbp)
    add     $4, %rbp
    jmp     12f
11: mov     %ecx, (%rbp)
    mov     %edx, 4(%rbp)
    add     $8, %rbp
12: mov     PACK_OUT_BASE(%rsp), %rax
    orb     $2, (%rax)                      /* HYBRID_BIT */
10:

    mov     %rbp, %rax
    sub     PACK_OUT_BASE(%rsp), %rax
    /* Packing that did not make the value smaller is packing that did not
     * help, and the caller stores the original. */
    cmp     PACK_SRC_LEN(%rsp), %rax
    jb      13f

KEVA_LABEL(pack, give_up):
    xor     %eax, %eax
13: add     $PACK_FRAME, %rsp
    pop     %r15
    pop     %r14
    pop     %r13
    pop     %r12
    pop     %rbp
    pop     %rbx
    ret

#if !defined(__APPLE__)
    .size KEVA_SYM(keva_pack), . - KEVA_SYM(keva_pack)
    .section .note.GNU-stack, "", %progbits
#endif
