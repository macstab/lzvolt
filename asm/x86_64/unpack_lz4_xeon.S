/*
 * The decoder for blocks somebody else wrote, on a Xeon.
 *
 * Same body as unpack.S today. It is a separate symbol so that making foreign
 * LZ4 blocks faster can never change what our own values execute -- see the
 * format axis in unpack.S.
 */
#define KEVA_LZ4 1
#define KEVA_XEON 1
#include "unpack.S"
