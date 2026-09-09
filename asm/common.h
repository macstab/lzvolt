/*
 * Shared assembler preprocessor helpers.
 *
 * Mach-O decorates C symbols with a leading underscore and ELF does not, so
 * every exported kernel name goes through KEVA_SYM().
 *
 * Deliberately *not* provided: a multi-line KEVA_FUNC_BEGIN() macro that emits
 * `.text`, `.p2align`, `.globl` and the label in one expansion. The C
 * preprocessor joins continuation lines into a single line, which forces `;` as
 * a statement separator -- and on AArch64 the integrated assembler silently
 * drops the directives after the first one. It does not warn, it does not fail
 * the build; you get an object file with no exported symbol and a link error
 * hundreds of lines later. Directives are written out per file instead. The
 * repetition is worth the absence of that failure mode.
 */

#ifndef KEVA_ASM_COMMON_H
#define KEVA_ASM_COMMON_H

#if defined(__APPLE__)
#  define KEVA_SYM(name) _##name
#else
#  define KEVA_SYM(name) name
#endif

/*
 * A label the assembler may move and shorten branches to.
 *
 * The convention is per object format and getting it wrong is silent: on ELF a
 * label starting with `.L` is assembler-temporary and never reaches the symbol
 * table, while Mach-O wants a plain `L`. Written `.L` for both, a Mach-O build
 * turns every one of them into a real symbol -- and the assembler then cannot
 * relax a branch across one, because a symbol's address is something someone
 * else might depend on.
 *
 * Measured on this decoder, assembling the same source for both:
 *
 *   ELF      61 branches, 29 of them two bytes, 1 symbol
 *   Mach-O   61 branches, 11 of them two bytes, 33 symbols
 *
 * Twenty-eight branches that could have been two bytes were six. It costs
 * nothing on the machines this is measured on, since those run ELF -- but it
 * meant the code tested here locally was not the code being measured.
 */
#if defined(__APPLE__)
#  define KEVA_LABEL(p, n) L ## p ## _ ## n
#else
#  define KEVA_LABEL(p, n) .L ## p ## _ ## n
#endif

#endif /* KEVA_ASM_COMMON_H */
