/*
 * The decoder, assembled a second time for the wide split.
 *
 * Two bits of literal length and six of match, against four and four. Which one
 * a value uses is in its header, and assembly takes that as a second copy of
 * the code rather than as a parameter: a register holding the token's shift
 * would sit on the critical path of every block for the whole length of every
 * value.
 *
 * It is a separate file rather than a second pass over the same one because two
 * compilations of one path collide on the object file they write.
 */
#define KEVA_WIDE 1
#include "unpack.S"
