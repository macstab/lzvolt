# lzvolt

[![crates.io](https://img.shields.io/crates/v/lzvolt.svg)](https://crates.io/crates/lzvolt)
[![docs.rs](https://img.shields.io/docsrs/lzvolt)](https://docs.rs/lzvolt)
[![CI](https://github.com/macstab/lzvolt/actions/workflows/ci.yml/badge.svg)](https://github.com/macstab/lzvolt/actions/workflows/ci.yml)
[![license](https://img.shields.io/crates/l/lzvolt.svg)](LICENSE)

**LZ, variable.** A byte-oriented LZ77 codec that picks its token layout per
value, with hand-written decoders assembled per CPU part line.

## 31% smaller *and* up to 83% faster than LZ4

Compression is supposed to be a trade — you buy ratio with time. This one takes
both ends. On 64 KiB records it writes **5524 bytes where liblz4 writes 7992**,
then reads them back **23–45% faster**. Those gains multiply:

| | capacity | × | decode | = | together |
|---|---|---|---|---|---|
| records 64 KiB, Xeon 8481C | **+44.7%** | | **+36%** | | **≈ +96%** |

<sub>1.447 × 1.36 = 1.97. Measured directly in the same run: +96.3%.</sub>

Across four machines that product is **+79% to +107%**. In the unit anyone
actually pays in: **a cache holding a given working set at a given read rate
needs about half the machines.**

### You can keep the data you already have

This decoder reads **LZ4's own blocks** — the bytes liblz4 already wrote — and
on 20 of 27 measured shapes it reads them faster than liblz4 does.

```c
/* stage one: change one call. Nothing is re-encoded, nothing is migrated. */
- LZ4_decompress_safe(src, dst, src_len, dst_cap);
+ lzvolt_decompress_lz4(src, src_len, dst, dst_cap, declared);
```

Stage two is re-encoding into the native format, when it suits you, for the
31% and the factor above.

**Compatibility runs one way only.** lzvolt reads LZ4; LZ4 cannot read lzvolt.
The native format's streams are not LZ4 blocks and a conforming LZ4 decoder
will reject some of them — see [the format](#the-format). There is no mode that
emits LZ4-compatible output.

```rust
let mut packed = Vec::new();
if lzvolt::compress(&value, &mut packed) {
    // a stream; it declares its own length
} else {
    // not worth packing — store the bytes as they are
}

let mut out = Vec::new();
lzvolt::decompress(&packed, &mut out)?;
```

```c
int64_t n = lzvolt_compress(data, len, buf, lzvolt_compress_bound(len));
if (n > 0)                             { store(buf, (size_t)n); }
else if (n == LZVOLT_NOT_COMPRESSIBLE) { store(data, len); }
else                                   { fail(lzvolt_strerror(n)); }
```

---

## The numbers

One standardised run: three machines, three runs each, medians of three, the
full test matrix executed before anything was measured. Every cell is this
codec against **liblz4 on its own format** — a decode against a decode, same
bytes, same machine, same process.

**Decoding our own format, GiB/s and the margin over liblz4:**

| shape | Xeon 8481C | EPYC 9B14 | Neoverse V2 |
|---|---|---|---|
| records 512 B | 9.44 **+29%** | 10.78 **+33%** | 10.60 **+22%** |
| varied 512 B | 8.24 **+29%** | 9.10 **+28%** | 8.47 **+27%** |
| records 4 KiB | 11.71 +18% | 12.63 +14% | 13.14 **+32%** |
| varied 4 KiB | 7.08 **+41%** | 8.40 **+37%** | 8.32 **+54%** |
| records 64 KiB | 11.86 **+36%** | 12.25 **+23%** | 12.93 **+45%** |
| varied 64 KiB | 7.01 **+72%** | 8.40 **+64%** | 8.15 **+83%** |

Eighteen cells of eighteen, +14% to +83%.

**Reading LZ4's own blocks**, the same decoder on bytes liblz4 wrote: ahead on
20 of 27 cells.

**Packing, GiB/s and the margin over liblz4:**

| shape | Xeon 8481C | EPYC 9B14 | Neoverse V2 |
|---|---|---|---|
| records 512 B | 2.00 **+55%** | 2.62 **+122%** | 2.23 **+54%** |
| varied 512 B | 1.25 **+58%** | 1.55 **+89%** | 1.22 +28% |
| noise 512 B | 2.43 **+81%** | 2.95 **+134%** | 2.87 **+77%** |
| records 4 KiB | 2.78 +19% | 3.60 +29% | 3.33 +22% |
| varied 4 KiB | 1.15 +20% | 1.47 +27% | 1.16 −2% |
| noise 4 KiB | 7.16 **+70%** | 7.74 **+76%** | 10.34 **+99%** |
| records 64 KiB | 3.14 +25% | 3.83 +20% | 3.56 +20% |
| varied 64 KiB | 1.25 **+92%** | 1.08 +0% | 1.08 +4% |
| noise 64 KiB | 114.6 **+859%** | 125.6 **+820%** | 165.1 **+837%** |

Twenty-five cells of twenty-seven. The 64 KiB noise row is **9.6×** and is a
different operation from the eight above it: lzvolt probes a value before
packing it and answers *"not worth compressing"* without running a match
search, where liblz4 compresses all 64 KiB and returns a block larger than the
input. Every cache performs that check; most bolt it on top of the codec and
pay for it twice. All comparisons call `LZ4_compress_default`, i.e.
acceleration 1.

**Packed size, bytes, against liblz4 and lz4_flex:**

| shape | lzvolt | liblz4 | lz4_flex | vs liblz4 |
|---|---|---|---|---|
| records 64 KiB | **5524** | 7992 | 7722 | **−30.9%** |
| records 4 KiB | **487** | 658 | 660 | **−26.0%** |
| varied 4 KiB | **2228** | 2333 | 2329 | −4.5% |
| varied 512 B | **356** | 366 | 368 | −2.7% |
| varied 64 KiB | 33180 | **32730** | 33005 | +1.4% |
| records 512 B | 195 | **192** | — | +1.6% |

Smaller on four shapes of six; larger on two, by 1.4% and 1.6%. Both are here
rather than left out.

Capacity and throughput are independent axes for a cache — more values per GB
of RAM, and each one read faster. On 64 KiB records of this shape the product
of the two is:

| | M2 Max | Xeon 8481C | EPYC 9B14 | Neoverse V2 |
|---|---|---|---|---|
| capacity × throughput | **+101.5%** | **+96.3%** | +79.1% | **+106.6%** |

Roughly twice the logical bytes per second per GB of RAM on three of the four.
(Measured in a separate four-machine run from the table above, which is why
the M2 appears here and not there.)

Every number here, how it was measured, and the roughly twenty ideas that
measured zero or negative, are in [docs/MEASUREMENTS.md](docs/MEASUREMENTS.md).
That file is the honest record, not a highlight reel.

### Where it loses

`noise` at 512 bytes in the foreign LZ4 format, by 12–15% on x86. Incompressible
512-byte blobs are an edge case — in our own format the packer refuses them
outright and there is no decode at all — but the deficit is real and the
mechanism is known: our kernel is more sensitive to buffer placement than
liblz4 there. It is written up in the measurement log rather than left out.

---

## Getting it

**Rust** — requires 1.85 or newer:

```toml
[dependencies]
lzvolt = "1.0"
```

**C, C++, or anything with an FFI:** the crate builds a `cdylib` and a
`staticlib`. The header is [`include/lzvolt.h`](include/lzvolt.h) and it is
hand-written, not generated, so it says what the functions mean and not just
what they take.

```sh
cargo build --release          # target/release/liblzvolt.{a,so,dylib}
make c-example                 # compiles examples/capi.c against it
```

Released binaries for glibc and musl on amd64 and arm64, and both macOS
slices, are attached to each
[release](https://github.com/macstab/lzvolt/releases) with `SHA256SUMS` — plus
`lzvolt.pc` and a CMake package config, so your build system can find the
library instead of you hard-coding paths:

```sh
make install PREFIX=/usr/local     # library, header, lzvolt.pc, CMake config
```

**pkg-config:**

```sh
cc myapp.c $(pkg-config --cflags --libs lzvolt) -o myapp
```

**CMake:**

```cmake
find_package(lzvolt 1.0 REQUIRED)
target_link_libraries(myapp PRIVATE lzvolt::lzvolt)
```

There is no C package registry to publish to — C has never had one, which is
why the release page *is* the distribution. `apt`, `brew`, vcpkg and Conan are
each a separate downstream packaging effort, and none of them is required to
use this.

---

## The API

Four entry points cover everything. The rest is behind `lzvolt::raw` and is
not stable.

| Rust | C | |
|---|---|---|
| `compress` | `lzvolt_compress` | returns `false` / `0` when not worth packing |
| `decompress` | `lzvolt_decompress` | the stream carries its own length |
| `decompress_into` | *(same)* | into a buffer you own, no copy |
| `decompress_lz4` | `lzvolt_decompress_lz4` | a raw LZ4 block |
| `backend` | `lzvolt_backend` | which code path this build decodes with |

Two things about that table are worth saying out loud.

**Compression may refuse, and that is a feature.** This encoder declines
anything it cannot shrink by an eighth, because reading a packed value costs
four to five times what reading a stored one does. A format that always wraps
its input makes you pay that on data it never helped. Store the original bytes
and skip the decode.

**Decompression writes into your buffer.** `decompress_into` in Rust and
`lzvolt_decompress` in C write straight into memory you already have, with no
copy afterwards. At 13 GiB/s a `memcpy` of the output would be a fifth of the
cost of the decode. Compression copies once internally; that is documented in
the header where a caller sees it.

---

## The format

Specified in [docs/FORMAT.md](docs/FORMAT.md) — normative, complete, and
written by reading the encoder and both decoders. Sixteen test vectors for a
second implementation are in [docs/vectors.txt](docs/vectors.txt), checked
against this decoder on every test run.

The short version:

- A stream is **self-contained**: it carries its own uncompressed length. No
  frame, no magic number, no checksum, no dictionary.
- The token's eight bits are split one of **two ways**, and a value may switch
  from one to the other **once**, partway through. The even split *is* LZ4's
  token layout, which is why an LZ4 block is a valid body here.
- Maximum value 2²⁹−1 bytes; window 64 KiB; minimum match 4.

**An LZ4 block decodes as an lzvolt body. The reverse is not true** — LZ4
constrains where a block may end and this format does not, so a conforming LZ4
decoder will reject some valid lzvolt streams, and it is right to.

---

## How it goes faster

Not by a cleverer algorithm. By three things, each of which was measured
before it was kept:

**A token layout chosen per value.** Most codecs fix the split between
literal-length and match-length bits at design time. This one carries two and
lets a value change its mind once. On 4 KiB records that is 19.2% off the
packed size and 15.1% on 64 KiB ones — and the bytes come out of *offsets*
alone: 58 of 96 blocks, and 493 of 1567, stop writing one at all because the
second layout spends its spare bit on a repeat flag. The wider match field pays
for itself separately, by removing chain bytes and the mispredict that comes
with them.

**Hand-written decoders, one per CPU part line.** Not one SIMD kernel with
runtime checks — separate assembled bodies for Xeon, EPYC, SSSE3-only x86,
generic AArch64 and Neoverse V2, selected once per process by CPUID or MIDR.
The same idea measured opposite signs on different parts often enough that
sharing a body was costing more than the duplication does: a memcpy threshold
that was right on Xeon cost EPYC 16%, and an alignment head worth +49% on
Neoverse V2 cost the M2 nothing at all.

**Removing work, not instructions.** The measurement log records about twenty
attempts to shorten a dependency chain or save an instruction. Nearly all of
them measured zero or negative. The three changes that carried — a `memcpy`
where a loop was (+34%), skipping a checked path entirely (+8%), one wide load
replacing three dependent ones (+37%) — all deleted work rather than
compressing it.

---

## Building and testing

```sh
make help          # every target, with what it does
make test          # the suite, both with and without the assembly
make check         # test + clippy + rustdoc + format, what CI runs
make quick         # a thumbs up/down benchmark in about seven seconds
make bench         # the long form, twenty minutes, for numbers worth quoting
```

`cargo test` alone works too. Some tests need the system liblz4 — the interop
soak, which puts 5760 liblz4-packed values through our kernel and compares byte
for byte. `build.rs` finds it via `pkg-config`, then `LZ4_LIB_DIR`, then the
usual prefixes.

### Which platforms get the assembly

| target | decoder |
|---|---|
| Linux and macOS on **x86-64** and **aarch64** | hand-written assembly, per part line |
| Windows, and every other architecture | portable Rust |

The assembly is GAS-syntax with ELF and Mach-O directives, so where the
assembler will not take it the portable decoder runs instead. Everything still
builds, passes the same tests and reads the same streams — it is slower, and
`lzvolt_backend()` says so rather than leaving you to guess. Extending the
kernels to COFF is work nobody has done; a patch would be welcome.

The assembly can also be turned off deliberately with `--no-default-features`, which
falls back to the portable reference. That is a supported configuration and it
is what the differential tests compare against, not a degraded mode.

**On correctness.** The decoders' copies are unchecked, so the validation in
front of them is the only thing between a malformed stream and memory
unsafety. They are held to that by fuzzing every truncation and every
single-bit flip of a packed stream, thousands of multi-byte corruptions,
streams that were never packed at all, differential testing against the
portable decoder and against liblz4, the whole set under Miri, and
AddressSanitizer over the assembly — which is the half Miri cannot execute.

---

## Credits

Created by **Christian Schnapka** ([@nolem](https://github.com/nolem)), and
developed inside [KevaVolt](https://github.com/macstab) before being extracted
and published on its own.

The measurement log in [docs/MEASUREMENTS.md](docs/MEASUREMENTS.md) is the
record of how it got here — including the roughly twenty ideas that measured
zero or negative, which are the more useful half.

## License

Apache-2.0. See [LICENSE](LICENSE) and [NOTICE](NOTICE).
