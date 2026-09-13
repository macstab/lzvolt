//! What the packer costs, in the units that decide whether to hand-write it.
//!
//! The comparison that matters is not against another implementation of this
//! algorithm but against LZ4, which is the same algorithm written by people who
//! have tuned it for a decade: roughly 500-750 MB/s packing and 3-5 GB/s
//! unpacking on one core. Landing near those numbers means the codegen is fine
//! and the remaining difference is algorithmic. Landing far below means there
//! is something to find — and only then is it worth looking at the assembly.
//!
//! Both directions are measured because they are used at completely different
//! rates. A read-heavy store unpacks ten times for every time it packs, so a
//! slow packer costs far less than the raw ratio suggests.

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use keva_core::store::pack;

/// Record-shaped, which is what a store actually holds. A run of identical
/// bytes would report a throughput no real value ever reaches, because the
/// match loop would never restart.
fn records(total: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(total);
    let mut i = 0u64;
    while out.len() < total {
        out.extend_from_slice(
            format!(
                "{{\"id\":{i},\"tenant\":\"tenant42\",\"active\":true,\"role\":\"member\",\
                 \"created\":\"2026-09-0{}T1{}:0{}:00Z\",\"score\":{},\"region\":\"eu-central-1\"}}",
                i % 9,
                i % 10,
                i % 10,
                i % 1000
            )
            .as_bytes(),
        );
        i += 1;
    }
    out.truncate(total);
    out
}

/// Records whose field *values* vary, not just an incrementing id.
///
/// The generator above repeats almost everything but a few digits, which
/// compresses far better than a real table of user records and inflates the
/// throughput, since that is reported against output bytes. This is the number
/// worth quoting.
fn varied(total: usize) -> Vec<u8> {
    let mut state = 0x2545_F491_4F6C_DD1Du64;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    let regions = ["eu-central-1", "us-east-1", "ap-south-1", "sa-east-1"];
    let roles = ["member", "admin", "viewer", "owner", "billing"];

    let mut out = Vec::with_capacity(total);
    while out.len() < total {
        let r = next();
        out.extend_from_slice(
            format!(
                "{{\"id\":\"{:016x}\",\"tenant\":\"{:08x}\",\"role\":\"{}\",\
                 \"region\":\"{}\",\"score\":{},\"seen\":{},\"token\":\"{:016x}\"}}",
                r,
                next() as u32,
                roles[(r % 5) as usize],
                regions[(r % 4) as usize],
                next() % 100_000,
                next() % 1_700_000_000,
                next()
            )
            .as_bytes(),
        );
    }
    out.truncate(total);
    out
}

/// Incompressible, so the match loop finds nothing and every byte becomes a
/// literal. This is the worst case for the packer and the case a namespace of
/// already-compressed blobs hits on every write.
fn noise(total: usize) -> Vec<u8> {
    let mut state = 0x2545_F491_4F6C_DD1Du64;
    (0..total)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state as u8
        })
        .collect()
}

fn packing(c: &mut Criterion) {
    let mut group = c.benchmark_group("pack/compress");

    for (label, data) in [
        ("records_512", records(512)),
        ("records_4k", records(4096)),
        ("records_64k", records(65_536)),
        ("varied_4k", varied(4096)),
        ("varied_64k", varied(65_536)),
        ("noise_4k", noise(4096)),
    ] {
        group.throughput(Throughput::Bytes(data.len() as u64));
        // Measured through the reusable packer, which is how a table calls it:
        // a fresh one per call would be measuring the setup, and the setup is
        // exactly what was removed.
        group.bench_with_input(BenchmarkId::from_parameter(label), &data, |b, data| {
            let mut out = Vec::with_capacity(data.len() * 2);
            let mut packer = pack::Packer::new();
            b.iter(|| black_box(packer.pack(black_box(data), &mut out)));
        });
    }

    group.finish();
}

/// The hand-written packer against the compiler's, over identical inputs.
///
/// The search kernel alone measured 4-17% ahead and still made the packer
/// 16-29% slower, because it was called once per match and an `extern "C"`
/// boundary stops the compiler carrying loop state in registers. This one
/// crosses once per value, so what shows up here is the kernel's real margin
/// rather than the boundary's cost.
fn packing_asm(c: &mut Criterion) {
    if !keva_asm::pack_find::asm_available() {
        return;
    }
    let mut group = c.benchmark_group("pack/asm");

    for (label, data) in [
        ("records_512", records(512)),
        ("records_4k", records(4096)),
        ("records_64k", records(65_536)),
        ("varied_4k", varied(4096)),
        ("varied_64k", varied(65_536)),
        ("noise_4k", noise(4096)),
    ] {
        group.throughput(Throughput::Bytes(data.len() as u64));

        group.bench_function(BenchmarkId::new("asm", label), |b| {
            let mut out = Vec::with_capacity(data.len() + 16);
            let mut table = keva_asm::pack_find::new_table();
            b.iter(|| {
                black_box(keva_asm::pack_find::pack_asm(
                    black_box(&data),
                    &mut out,
                    &mut table,
                ))
            });
        });

        group.bench_function(BenchmarkId::new("rust", label), |b| {
            let mut out = Vec::with_capacity(data.len() * 2);
            let mut packer = pack::Packer::new();
            b.iter(|| black_box(packer.pack(black_box(&data), &mut out)));
        });
    }

    group.finish();
}

/// Where packing peaks, and whether unpacking peaks in the same place.
///
/// Splitting a value into blocks would let any size run at whatever size turns
/// out to be fastest, so this is the measurement that picks the block size --
/// and it has to agree in both directions, since a block that packs fast and
/// unpacks slowly is no use to a store that reads far more than it writes.
///
/// `varied` is the shape that discriminates: it is compressible enough that the
/// packer does real work, and varied enough that the match table cannot hold the
/// whole input once it grows.
fn sizes(c: &mut Criterion) {
    let mut group = c.benchmark_group("pack/sizes");

    for bytes in [512usize, 1024, 2048, 4096, 8192, 16_384, 32_768, 65_536] {
        let data = varied(bytes);
        group.throughput(Throughput::Bytes(bytes as u64));

        group.bench_function(BenchmarkId::new("pack", bytes), |b| {
            let mut out = Vec::with_capacity(bytes + 16);
            let mut table = keva_asm::pack_find::new_table();
            b.iter(|| {
                black_box(keva_asm::pack_find::pack_asm(
                    black_box(&data),
                    &mut out,
                    &mut table,
                ))
            });
        });

        let mut packed = Vec::new();
        if !pack::pack(&data, &mut packed) {
            continue;
        }
        group.bench_function(BenchmarkId::new("unpack", bytes), |b| {
            let mut out = Vec::with_capacity(bytes);
            b.iter(|| pack::unpack(black_box(&packed), &mut out).unwrap());
        });
    }

    group.finish();
}

fn unpacking(c: &mut Criterion) {
    let mut group = c.benchmark_group("pack/decompress");

    for (label, data) in [
        ("records_512", records(512)),
        ("records_4k", records(4096)),
        ("records_64k", records(65_536)),
        ("varied_4k", varied(4096)),
        ("varied_64k", varied(65_536)),
    ] {
        let mut packed = Vec::new();
        if !pack::pack(&data, &mut packed) {
            continue;
        }
        // Throughput is reported against the original size, since that is what
        // the caller receives and what a competing library would report.
        group.throughput(Throughput::Bytes(data.len() as u64));
        group.bench_with_input(BenchmarkId::from_parameter(label), &packed, |b, packed| {
            let mut out = Vec::with_capacity(data.len());
            b.iter(|| pack::unpack(black_box(packed), &mut out).unwrap());
        });
    }

    group.finish();
}

/// The ratio, alongside the speed, because one is meaningless without the
/// other — a packer can always be made faster by compressing less.
fn ratio(c: &mut Criterion) {
    for (label, data) in [
        ("records_4k", records(4096)),
        ("varied_4k", varied(4096)),
        ("varied_64k", varied(65_536)),
        ("noise_4k", noise(4096)),
    ] {
        let mut packed = Vec::new();
        let kept = pack::pack(&data, &mut packed);
        eprintln!(
            "  ratio {label}: {} -> {} bytes ({:.2}x, {})",
            data.len(),
            packed.len(),
            data.len() as f64 / packed.len() as f64,
            if kept { "kept" } else { "discarded" }
        );
    }
    // Nothing to time here; the group exists so the numbers appear in the run.
    c.bench_function("pack/ratio_noop", |b| b.iter(|| black_box(0u8)));
}

/// The same data through LZ4, in the same harness.
///
/// Every comparison in this file was against numbers quoted from LZ4's
/// documentation, measured on other hardware with other data. This runs it
/// here, so the difference is the implementation and not the conditions.
fn against_lz4(c: &mut Criterion) {
    let mut group = c.benchmark_group("lz4");

    // The same shapes the decompress benchmark uses, all of them. This list was
    // missing records_512 and records_64k, which are the two where our own
    // format wins hardest -- so the best number in the file had nothing to be
    // compared against, and the comparison flattered us by omission.
    for (label, data) in [
        ("records_512", records(512)),
        ("varied_4k", varied(4096)),
        ("varied_64k", varied(65_536)),
        ("records_4k", records(4096)),
        ("records_64k", records(65_536)),
        ("noise_4k", noise(4096)),
    ] {
        group.throughput(Throughput::Bytes(data.len() as u64));

        group.bench_function(BenchmarkId::new("compress", label), |b| {
            b.iter(|| black_box(lz4_flex::compress(black_box(&data))));
        });

        let packed = lz4_flex::compress(&data);
        let original = data.len();
        // Into a buffer that outlives the loop, because our own decompress
        // benchmark reuses its output too. `decompress` would allocate and free
        // a `Vec` inside every iteration and charge LZ4 for it.
        let mut out = vec![0u8; original + 64];
        group.bench_function(BenchmarkId::new("decompress", label), |b| {
            b.iter(|| {
                black_box(lz4_flex::block::decompress_into(
                    black_box(&packed),
                    &mut out,
                ))
            });
        });

        eprintln!(
            "  lz4 ratio {label}: {} -> {} bytes ({:.2}x)",
            original,
            packed.len(),
            original as f64 / packed.len() as f64
        );
    }

    group.finish();
}

/// Both decoders over the identical LZ4 block.
///
/// Every other comparison in this file has a format difference somewhere in it:
/// ours carries a length header, theirs does not, and the two packers make
/// different blocks out of the same input. Here the bytes are the ones liblz4
/// produced and each side is handed the uncompressed length the same way, so
/// what is left is the decoder and nothing else.
///
///   RUSTFLAGS="-L/opt/homebrew/lib" cargo bench -p keva-core --features liblz4
#[cfg(feature = "liblz4")]
fn same_bytes(c: &mut Criterion) {
    #[link(name = "lz4")]
    extern "C" {
        fn LZ4_compress_default(s: *const u8, d: *mut u8, n: i32, cap: i32) -> i32;
        fn LZ4_decompress_safe(s: *const u8, d: *mut u8, n: i32, cap: i32) -> i32;
    }

    let mut group = c.benchmark_group("same_bytes");
    // The small sizes are here and not only in the format comparison, because
    // that comparison cannot be fair at this end. `lz4_flex::decompress_into`
    // is `#[inline]` Rust, so the compiler folds it into the loop and saves
    // only the registers it touches; our decoder is an `extern "C"` call into
    // assembly with a twelve-register prologue that can never be elided. At 512
    // bytes the fixed cost of a call is 41% of the work, so that difference is
    // most of what such a comparison measures. liblz4 is the like-for-like
    // opponent: also a C call, also a real ABI prologue.
    // The nine the packing benchmark uses, so the three tables line up shape for
    // shape and nothing goes unwatched. Noise belongs here even though our
    // packer declines to pack it: liblz4 writes a literal block for it, and
    // decoding a literal block is a real measurement of the literal path --
    // which is the one every incompressible value takes.
    for (label, data) in [
        ("records_512", records(512)),
        ("varied_512", varied(512)),
        ("noise_512", noise(512)),
        ("records_4k", records(4096)),
        ("varied_4k", varied(4096)),
        ("noise_4k", noise(4096)),
        ("records_64k", records(65_536)),
        ("varied_64k", varied(65_536)),
        ("noise_64k", noise(65_536)),
    ] {
        let mut block = vec![0u8; data.len() + 1024];
        let n = unsafe {
            LZ4_compress_default(
                data.as_ptr(),
                block.as_mut_ptr(),
                data.len() as i32,
                block.len() as i32,
            )
        };
        assert!(n > 0);
        block.truncate(n as usize);
        let mut theirs = vec![0u8; data.len() + 64];
        let mut flex = vec![0u8; data.len() + 64];
        let mut mine = vec![0u8; data.len() + 64];

        group.throughput(Throughput::Bytes(data.len() as u64));
        // The same shape of call on both sides: a source, a destination and its
        // size. No allocation, no Vec bookkeeping, no Result on either.
        // Checked once, outside the loop, so the timed body is the same shape
        // on both sides: one call, and the result handed to `black_box`.
        // The LZ4 body, which is what a foreign block reaches in production
        // too. Identical to the even body today; the point of the separation is
        // that it stops being so without our own format's timings moving.
        assert!(keva_asm::unpack::unpack_lz4_into_slice(
            &block,
            &mut mine,
            data.len()
        ));
        group.bench_function(BenchmarkId::new("keva", label), |b| {
            b.iter(|| {
                black_box(keva_asm::unpack::unpack_lz4_into_slice(
                    black_box(&block),
                    &mut mine,
                    data.len(),
                ))
            });
        });
        group.bench_function(BenchmarkId::new("liblz4", label), |b| {
            b.iter(|| unsafe {
                black_box(LZ4_decompress_safe(
                    black_box(block.as_ptr()),
                    theirs.as_mut_ptr(),
                    block.len() as i32,
                    data.len() as i32,
                ))
            });
        });
        // `decompress_into` rather than `decompress`, so this arm reuses its
        // buffer like the other two instead of allocating one per call. The
        // crate is built without `safe-decode`, which is its fast path.
        assert_eq!(
            lz4_flex::block::decompress_into(&block, &mut flex).unwrap(),
            data.len()
        );
        group.bench_function(BenchmarkId::new("lz4_flex", label), |b| {
            b.iter(|| {
                black_box(lz4_flex::block::decompress_into(
                    black_box(&block),
                    &mut flex,
                ))
            });
        });
    }
    group.finish();
}

/// Each side on its own format, and each through a bare call into a library.
///
/// This is the comparison a user of either one actually gets, and nothing here
/// had made it against the reference implementation -- the `lz4` group uses
/// lz4_flex, which is a Rust port and gets inlined. Both arms here are
/// `extern "C"` calls with a real ABI prologue, both write into a buffer that
/// outlives the loop, and both are handed the uncompressed length. What differs
/// is the format and the packer that produced it, which is the point.
#[cfg(feature = "liblz4")]
fn own_format(c: &mut Criterion) {
    #[link(name = "lz4")]
    extern "C" {
        fn LZ4_compress_default(s: *const u8, d: *mut u8, n: i32, cap: i32) -> i32;
        fn LZ4_decompress_safe(s: *const u8, d: *mut u8, n: i32, cap: i32) -> i32;
    }

    let mut group = c.benchmark_group("own_format");
    // All nine, including the three the packer refuses. Leaving those out was
    // reporting the wrong thing: the format reads a block that is all literals
    // perfectly well -- there is a test that builds one by hand for every
    // boundary between 1 and 65536 bytes -- what does not exist is a noise
    // block *we wrote*. `index/table.rs` keeps a refused value raw, with
    // `compressed: false`, and its reader copies the bytes out without a
    // decoder running at all. That is the cell, and it is a real one: the
    // reader's cost for noise, against what liblz4 and lz4_flex charge to
    // decode a literal block for the same result.
    for (label, data) in [
        ("records_512", records(512)),
        ("varied_512", varied(512)),
        ("noise_512", noise(512)),
        ("records_4k", records(4096)),
        ("varied_4k", varied(4096)),
        ("noise_4k", noise(4096)),
        ("records_64k", records(65_536)),
        ("varied_64k", varied(65_536)),
        ("noise_64k", noise(65_536)),
    ] {
        // Ours: the packed body with the header stripped, so the kernel is
        // handed exactly what liblz4's is -- a block and a length.
        let mut ours = Vec::new();
        let packed = pack::pack(&data, &mut ours);
        let (body, split, other, switch) = if packed {
            let (raw, header) = keva_core::store::entry::get_varint(&ours).expect("a header");
            let split = if raw & 1 == 1 {
                keva_asm::unpack::Split::WideMatch
            } else {
                keva_asm::unpack::Split::Even
            };
            let other = if split == keva_asm::unpack::Split::Even {
                keva_asm::unpack::Split::WideMatch
            } else {
                keva_asm::unpack::Split::Even
            };
            // A value that changes split partway is two calls, and it is
            // measured as two -- that is what its reader actually pays.
            let (body, switch) = if raw & 0b10 != 0 {
                let w = if data.len() <= 0x1_0000 { 2 } else { 4 };
                let cut = ours.len() - 2 * w;
                let rd = |b: &[u8]| -> usize {
                    if b.len() == 2 {
                        u16::from_le_bytes(b.try_into().unwrap()) as usize
                    } else {
                        u32::from_le_bytes(b.try_into().unwrap()) as usize
                    }
                };
                let in_at = rd(&ours[cut..cut + w]);
                let out_at = rd(&ours[cut + w..]);
                (ours[header..cut].to_vec(), Some((in_at, out_at)))
            } else {
                (ours[header..].to_vec(), None)
            };
            (body, split, other, switch)
        } else {
            // Nothing is read from these; the raw arm below never looks at
            // them. They exist so the decode closure has one shape.
            (
                Vec::new(),
                keva_asm::unpack::Split::Even,
                keva_asm::unpack::Split::Even,
                None,
            )
        };

        let mut theirs = vec![0u8; data.len() + 1024];
        let n = unsafe {
            LZ4_compress_default(
                data.as_ptr(),
                theirs.as_mut_ptr(),
                data.len() as i32,
                theirs.len() as i32,
            )
        };
        assert!(n > 0);
        theirs.truncate(n as usize);

        let mut mine = vec![0u8; data.len() + 64];
        let mut yours = vec![0u8; data.len() + 64];
        let decode = |body: &[u8], mine: &mut [u8]| match switch {
            // A value the packer refused is stored as it arrived, so reading it
            // is a copy of its bytes and no more. Measuring anything else here
            // would be measuring a path this store never takes.
            _ if !packed => {
                mine[..data.len()].copy_from_slice(&data);
                true
            }
            None => keva_asm::unpack::unpack_into_slice(body, mine, data.len(), split),
            Some((in_at, out_at)) => {
                keva_asm::unpack::unpack_section(&body[..in_at], mine, out_at, 0, split)
                    && keva_asm::unpack::unpack_section(
                        &body[in_at..],
                        mine,
                        data.len(),
                        out_at,
                        other,
                    )
            }
        };
        assert!(decode(&body, &mut mine));

        group.throughput(Throughput::Bytes(data.len() as u64));
        group.bench_function(BenchmarkId::new("keva", label), |b| {
            b.iter(|| black_box(decode(black_box(&body), &mut mine)));
        });
        group.bench_function(BenchmarkId::new("liblz4", label), |b| {
            b.iter(|| unsafe {
                black_box(LZ4_decompress_safe(
                    black_box(theirs.as_ptr()),
                    yours.as_mut_ptr(),
                    theirs.len() as i32,
                    data.len() as i32,
                ))
            });
        });
        // The Rust port on its own blocks too. It is `#[inline]`, so the
        // compiler folds it into the loop where the other two pay for a call --
        // an advantage that is real for its users and worth naming rather than
        // engineering away.
        let flexed = lz4_flex::compress(&data);
        let mut flexout = vec![0u8; data.len() + 64];
        assert_eq!(
            lz4_flex::block::decompress_into(&flexed, &mut flexout).unwrap(),
            data.len()
        );
        group.bench_function(BenchmarkId::new("lz4_flex", label), |b| {
            b.iter(|| {
                black_box(lz4_flex::block::decompress_into(
                    black_box(&flexed),
                    &mut flexout,
                ))
            });
        });
        eprintln!(
            "  own_format {label}: keva {} B{}, liblz4 {} B",
            if packed { ours.len() } else { data.len() },
            if !packed {
                " (refused, stored raw)"
            } else if switch.is_some() {
                " (two sections)"
            } else {
                ""
            },
            theirs.len()
        );
    }
    group.finish();
}

#[cfg(not(feature = "liblz4"))]
fn own_format(_: &mut Criterion) {}

/// The whole read, not the kernel: what a caller of `pack::unpack` pays.
///
/// `own_format` strips the header outside the timed loop and hands the kernel a
/// bare body, so every number this project has published for our own format is
/// a kernel time. Production does not get that: `store/pack.rs` reads the
/// varint, derives the split, looks for a hybrid trailer and range-checks both
/// of its offsets before the kernel is entered -- in Rust, once per value, on
/// the read path of a store that reads ten times for every write.
///
/// Nothing here had ever measured it. The difference between this group and
/// `own_format` is what that framing costs, and it is the number that decides
/// whether the header belongs in the kernel.
///
/// The other two are end-to-end in the same sense: bytes in, value out,
/// destination buffer reused across iterations so the comparison is decoding
/// and not allocation. A value the packer refused is copied, because that is
/// what the store does with it.
#[cfg(feature = "liblz4")]
fn production(c: &mut Criterion) {
    #[link(name = "lz4")]
    extern "C" {
        fn LZ4_compress_default(s: *const u8, d: *mut u8, n: i32, cap: i32) -> i32;
        fn LZ4_decompress_safe(s: *const u8, d: *mut u8, n: i32, cap: i32) -> i32;
    }

    let mut group = c.benchmark_group("production");
    for (label, data) in [
        ("records_512", records(512)),
        ("varied_512", varied(512)),
        ("noise_512", noise(512)),
        ("records_4k", records(4096)),
        ("varied_4k", varied(4096)),
        ("noise_4k", noise(4096)),
        ("records_64k", records(65_536)),
        ("varied_64k", varied(65_536)),
        ("noise_64k", noise(65_536)),
    ] {
        let mut ours = Vec::new();
        let packed = pack::pack(&data, &mut ours);

        let mut theirs = vec![0u8; data.len() + 1024];
        let n = unsafe {
            LZ4_compress_default(
                data.as_ptr(),
                theirs.as_mut_ptr(),
                data.len() as i32,
                theirs.len() as i32,
            )
        };
        assert!(n > 0);
        theirs.truncate(n as usize);
        let flexed = lz4_flex::compress(&data);

        let mut mine = Vec::with_capacity(data.len() + 64);
        let mut yours = vec![0u8; data.len() + 64];
        let mut flexout = vec![0u8; data.len() + 64];

        if packed {
            pack::unpack(&ours, &mut mine).expect("our own block");
            assert_eq!(mine, data);
        }

        group.throughput(Throughput::Bytes(data.len() as u64));
        group.bench_function(BenchmarkId::new("keva", label), |b| {
            if packed {
                b.iter(|| black_box(pack::unpack(black_box(&ours), &mut mine).is_ok()));
            } else {
                // Refused, so the store holds the bytes as they arrived and the
                // reader copies them out. No header, no kernel.
                b.iter(|| {
                    mine.clear();
                    mine.extend_from_slice(black_box(&data));
                    black_box(mine.len())
                });
            }
        });
        group.bench_function(BenchmarkId::new("liblz4", label), |b| {
            b.iter(|| unsafe {
                black_box(LZ4_decompress_safe(
                    black_box(theirs.as_ptr()),
                    yours.as_mut_ptr(),
                    theirs.len() as i32,
                    data.len() as i32,
                ))
            });
        });
        group.bench_function(BenchmarkId::new("lz4_flex", label), |b| {
            b.iter(|| {
                black_box(lz4_flex::block::decompress_into(
                    black_box(&flexed),
                    &mut flexout,
                ))
            });
        });
    }
    group.finish();
}

#[cfg(not(feature = "liblz4"))]
fn production(_: &mut Criterion) {}

/// All three packers over the same input, in one run.
///
/// The packing comparison had only ever been against lz4_flex, which is a Rust
/// port; the reference implementation was never in it. Each writes into a
/// destination that outlives the loop, so no arm is charged for an allocation
/// the others do not make.
///
/// One asymmetry is left standing because it is real: ours writes through a
/// `Vec`, which is its API, and profiling puts about 9% of the packer in that
/// bookkeeping. The other two write into a slice.
#[cfg(feature = "liblz4")]
fn three_packers(c: &mut Criterion) {
    #[link(name = "lz4")]
    extern "C" {
        fn LZ4_compress_default(s: *const u8, d: *mut u8, n: i32, cap: i32) -> i32;
    }

    let mut group = c.benchmark_group("compress3");
    // Three shapes at 512 bytes, not one. A cache stores short values far more
    // often than long ones, so that is the size any claim about this packer
    // rests on -- and it rested on `records` alone, which is the shape it
    // handles best.
    for (label, data) in [
        ("records_512", records(512)),
        ("varied_512", varied(512)),
        ("noise_512", noise(512)),
        ("records_4k", records(4096)),
        ("records_64k", records(65_536)),
        ("varied_4k", varied(4096)),
        ("varied_64k", varied(65_536)),
        ("noise_4k", noise(4096)),
        // A large value that will not compress: an image, a ciphertext, an
        // already packed blob. All three packers give up on it -- the question
        // is how much of the value each one reads before it does.
        ("noise_64k", noise(65_536)),
    ] {
        group.throughput(Throughput::Bytes(data.len() as u64));

        let mut ours = Vec::with_capacity(data.len() * 2);
        let mut packer = pack::Packer::new();
        packer.pack(&data, &mut ours);
        let keva_len = ours.len();
        group.bench_function(BenchmarkId::new("keva", label), |b| {
            b.iter(|| black_box(packer.pack(black_box(&data), &mut ours)));
        });

        let mut cbuf = vec![0u8; lz4_flex::block::get_maximum_output_size(data.len())];
        let n = unsafe {
            LZ4_compress_default(
                data.as_ptr(),
                cbuf.as_mut_ptr(),
                data.len() as i32,
                cbuf.len() as i32,
            )
        };
        assert!(n > 0);
        let lib_len = n as usize;
        group.bench_function(BenchmarkId::new("liblz4", label), |b| {
            b.iter(|| unsafe {
                black_box(LZ4_compress_default(
                    black_box(data.as_ptr()),
                    cbuf.as_mut_ptr(),
                    data.len() as i32,
                    cbuf.len() as i32,
                ))
            });
        });

        let flex_len =
            lz4_flex::block::compress_into(&data, &mut cbuf).expect("sized by its own bound");
        group.bench_function(BenchmarkId::new("lz4_flex", label), |b| {
            b.iter(|| black_box(lz4_flex::block::compress_into(black_box(&data), &mut cbuf)));
        });

        eprintln!(
            "  compress3 {label}: keva {keva_len} B, liblz4 {lib_len} B, lz4_flex {flex_len} B"
        );
    }
    group.finish();
}

#[cfg(not(feature = "liblz4"))]
fn three_packers(_: &mut Criterion) {}

#[cfg(not(feature = "liblz4"))]
fn same_bytes(_: &mut Criterion) {}

criterion_group!(
    benches,
    packing,
    packing_asm,
    sizes,
    unpacking,
    ratio,
    against_lz4,
    same_bytes,
    own_format,
    production,
    three_packers
);
criterion_main!(benches);
