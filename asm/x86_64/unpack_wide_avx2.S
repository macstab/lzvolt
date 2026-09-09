/*
 * The 256-bit decoder, assembled for the wide split -- two bits of literal
 * length and six of match. Four kernels on x86 rather than two: the split is
 * compiled in because a register holding the token's shift would sit on the
 * critical path of every block, and the vector width is compiled in because
 * dispatching per block would cost more than it saves.
 */
#define KEVA_AVX2 1
#define KEVA_WIDE 1
#include "unpack.S"
