# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

**The stream format is versioned separately and independently.** A major
version of this library may change its API; it will not change what
[docs/FORMAT.md](docs/FORMAT.md) specifies. Streams outlive releases.

## [1.0.0] — 2026-09-27

First release. The codec was developed inside a key-value store, measured
across four processors over several weeks, and extracted once it was clear it
was worth publishing on its own.

### The codec

- **Two token layouts, chosen per value.** A stream may switch from one to the
  other once, partway through. Worth about 19% where match lengths cluster
  past the narrow field's ceiling.
- **Hand-written decoders per CPU part line** — Xeon, EPYC, SSSE3-only x86,
  generic AArch64, Neoverse V2 — selected once per process by CPUID or MIDR.
  Separate assembled bodies rather than one kernel with runtime branches,
  because the same change kept measuring opposite signs on different parts.
- **An encoder that refuses.** Anything it cannot shrink by an eighth is
  handed back untouched, because reading a packed value costs four to five
  times what reading a stored one does.
- **Reads raw LZ4 blocks**, faster than liblz4 does on 20 of 27 measured
  cells. The even token split is LZ4's, so an LZ4 block is a valid body here.
  The reverse does not hold and is not claimed.

### Performance

Against liblz4 on its own format, three machines, medians of three, full test
matrix run before measuring:

- Decoding our own format: **ahead on 18 of 18 cells, +14% to +83%.**
- Packing: ahead on 25 of 27, and smaller.
- Capacity × throughput on 64 KiB records: **+96% to +107%.**

Every number, and the roughly twenty ideas that measured zero or negative, are
in [docs/MEASUREMENTS.md](docs/MEASUREMENTS.md).

### Interfaces

- **Rust:** `compress`, `decompress`, `decompress_into`, `decompress_lz4`,
  `backend`. Everything else is behind `raw` and is not stable.
- **C:** [`include/lzvolt.h`](include/lzvolt.h), hand-written. Buffers belong
  to the caller and nothing needs freeing. Decompression writes straight into
  your buffer with no copy; compression copies its result once, which the
  header says where a caller will see it.
- Builds as `rlib`, `staticlib` and `cdylib`.

### Format and conformance

- [docs/FORMAT.md](docs/FORMAT.md) — normative specification, version 1.
- [docs/vectors.txt](docs/vectors.txt) — sixteen decoder test vectors, checked
  against this decoder on every test run.
- A header must use the shortest class that holds its length, so one length
  has exactly one encoding and packed bytes can be compared for equality.

### Correctness

Every truncation and every single-bit flip of a packed stream; thousands of
multi-byte corruptions; streams that were never packed at all; differential
tests against the portable decoder and against liblz4; 5760 liblz4-packed
values compared byte for byte; the whole set under Miri.

The assembly is additionally held to mutation testing: a deliberate corruption
has to turn the suite red before a change is accepted. Two real bugs were
found that way — an entry bias that read 63 bytes past the input buffer while
still producing correct output, and a decoder that executed its own shuffle
tables as instructions on a match shape the benchmarks never produced.
