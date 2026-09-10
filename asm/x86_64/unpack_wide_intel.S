/*
 * The decoder for Intel parts, wide split.
 *
 * A separate object rather than a second pass: two compilations of one
 * path collide on the file they write. See unpack.S for what differs.
 */
#define KEVA_WIDE 1
#define KEVA_INTEL 1
#include "unpack.S"
