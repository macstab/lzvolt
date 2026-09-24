/*
 * The foreign-block decoder, assembled a second time for Neoverse V2.
 *
 * The first part line on this architecture. Until now one body per split served
 * every AArch64 part, which held while every measurement agreed and stopped
 * holding when one did not: sixty-four bytes a round in the literal loop is
 * worth +7.9% on an M2 Max and -1.4% on a V2. One body cannot answer both.
 *
 * Only the LZ4 body is doubled. Our own format declines to pack incompressible
 * data, so the long literal run this line is about does not arise there.
 */
#define KEVA_LZ4 1
#define KEVA_NEOVERSE 1
#include "unpack.S"
