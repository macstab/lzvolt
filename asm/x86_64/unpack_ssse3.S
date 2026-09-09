/*
 * The decoder again, with the pattern shuffle.
 *
 * Same format, same bounds, same branches. The only difference is how a match
 * closer than sixteen bytes is grown: `pshufb` builds the whole window from the
 * bytes behind the cursor instead of copying them one at a time until the gap
 * is wide enough for vectors.
 *
 * SSSE3 is not part of the x86-64 baseline -- it arrived with Core 2 and
 * Barcelona -- so this one is chosen at run time. A separate object file rather
 * than a second pass, because two compilations of one path collide on the file
 * they write.
 */
#define KEVA_SSSE3 1
#include "unpack.S"
