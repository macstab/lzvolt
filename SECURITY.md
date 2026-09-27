# Security

## Reporting a vulnerability

Report privately through GitHub's
[security advisory form](https://github.com/macstab/lzvolt/security/advisories/new).
Please do not open a public issue for a vulnerability.

Include what you have: the input that triggers it, the target and build
configuration (`lzvolt_backend()` prints the code path), and whatever you
know about the impact. A reproducer is worth more than an analysis.

You should get an acknowledgement within three working days and an assessment
within ten. If a fix is warranted it goes out as a patch release with an
advisory naming you, unless you would rather not be named.

## What counts

This library decodes bytes that may come from anywhere. Its decoders use
unchecked copies, so the validation in front of them is the only thing between
a malformed stream and memory unsafety.

**In scope**, and treated as serious:

- Any read or write outside a buffer, on any input, valid or not.
- A panic or abort reachable from a malformed stream. The decoders return
  errors; they do not unwind, and across the C ABI an unwind would abort the
  process.
- A stream that decodes to a length other than the one it declares.
- Unbounded allocation driven by a header field.
- Anything that is undefined behaviour under Miri or the sanitisers.

**Out of scope**, because the format does not claim them:

- *Undetected corruption.* There is no checksum. Flipping a byte in a stream
  can produce a different stream that is entirely valid, and the decoder will
  decode it. If you need to know the bytes are the ones you wrote, put a
  checksum above this layer. See `docs/FORMAT.md`.
- *Decompression bombs by ratio alone.* A stream can declare up to 2²⁹−1 bytes
  and the decoder will try to produce them. The declared length is readable
  before decoding with `decompressed_size` / `lzvolt_declared_size`; refuse
  what your own budget cannot hold.
- *Timing variation with data.* This is a compression codec tuned for speed,
  not a constant-time primitive. Do not use it on secrets where the
  compressed size or the decode time is observable — that is the CRIME class
  of attack and it applies to every compressor.

## How the decoders are checked

Every release runs, on each supported target: every truncation and every
single-bit flip of a packed stream; thousands of multi-byte corruptions;
streams that were never packed at all; differential tests against the portable
decoder and against liblz4; and 5760 liblz4-packed values compared byte for
byte.

Two things check the `unsafe` itself, and they divide the work because one of
them is slow:

- **AddressSanitizer** runs on every change and does cover the assembly,
  which is the half Miri cannot execute. There is no UBSan to pair it with —
  Rust does not have one, whatever a checklist may suggest — so the other half
  is `-Zub-checks`, which turns on the standard library's own UB assertions
  under release optimisation.
- **Miri** runs on every change at reduced volume, and at full volume on a
  schedule. The reduction is deliberate and measured: one test that takes a
  hundredth of a second natively took **2229 seconds** under Miri, almost all
  of it in a single case that decodes 2 MiB through the overlapping-copy path.
  That time bought no coverage — Miri checks pointer arithmetic and
  provenance, and a 300-byte match exercises the same branches. The tests keep
  their shape under it and lose their bulk; `make miri-full` is the unreduced
  run, and the reduced one asserts how many cases it skipped so that the
  skipping cannot grow unnoticed.

A change to the assembly is not accepted on a green test run alone — a
deliberate corruption of it has to turn the suite red first. Two bugs were
found by exactly that and are written up in `docs/MEASUREMENTS.md`.

## Supported versions

The most recent minor release gets security fixes. There are no older
supported branches yet; this will change when there is more than one.
