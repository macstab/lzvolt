# lzvolt
Fastest LZ77 compression codec in Assembler with a Rust/C API. Picks its token layout per value, uses hand-written decoders per CPU, packs smaller and decodes 14-83% faster than LZ4, and also reads raw LZ4 blocks.
