/*
 * The decoder for AMD parts, even split.
 *
 * A separate object rather than a second pass: two compilations of one
 * path collide on the file they write. See unpack.S for what differs.
 */
#define KEVA_AMD 1
#include "unpack.S"
