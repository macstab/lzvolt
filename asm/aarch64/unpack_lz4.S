/*
 * The decoder for blocks somebody else wrote.
 *
 * Same body as unpack.S today. It is a separate symbol so that making foreign
 * LZ4 blocks faster can never change what our own values execute -- see the
 * format axis in the x86-64 unpack.S, which this mirrors.
 */
#define KEVA_LZ4 1
#include "unpack.S"
