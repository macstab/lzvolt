/*
 * The decoder again, with 256-bit moves.
 *
 * The algorithm does not change: same format, same bounds, same branches. Only
 * the fixed match move and the block copies get wider, which on x86 is the
 * difference between two 128-bit pairs and one 256-bit pair -- four micro-ops
 * against two, and six against twelve under the wide split.
 *
 * AVX2 is not part of the x86-64 baseline, so this one is chosen at run time.
 * It is a separate object file rather than a second pass because two
 * compilations of one path collide on the file they write.
 */
#define KEVA_AVX2 1
#include "unpack.S"
