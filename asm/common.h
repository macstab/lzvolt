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

#endif /* KEVA_ASM_COMMON_H */
