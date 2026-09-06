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
            b.iter(|| black_box(lz4_flex::block::decompress_into(black_box(&packed), &mut out)));
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
    for (label, data) in [
        ("varied_4k", varied(4096)),
        ("varied_64k", varied(65_536)),
        ("records_4k", records(4096)),
        ("records_64k", records(65_536)),
    ] {
        let mut block = vec![0u8; data.len() + 1024];
        let n = unsafe {
            LZ4_compress_default(data.as_ptr(), block.as_mut_ptr(), data.len() as i32, block.len() as i32)
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
        assert!(keva_asm::unpack::unpack_into_slice(&block, &mut mine, data.len(), keva_asm::unpack::Split::Even));
        group.bench_function(BenchmarkId::new("keva", label), |b| {
            b.iter(|| {
                black_box(keva_asm::unpack::unpack_into_slice(
                    black_box(&block), &mut mine, data.len(), keva_asm::unpack::Split::Even))
            });
        });
        group.bench_function(BenchmarkId::new("liblz4", label), |b| {
            b.iter(|| unsafe {
                black_box(LZ4_decompress_safe(black_box(block.as_ptr()), theirs.as_mut_ptr(),
                                              block.len() as i32, data.len() as i32))
            });
        });
        // `decompress_into` rather than `decompress`, so this arm reuses its
        // buffer like the other two instead of allocating one per call. The
        // crate is built without `safe-decode`, which is its fast path.
        assert_eq!(lz4_flex::block::decompress_into(&block, &mut flex).unwrap(), data.len());
        group.bench_function(BenchmarkId::new("lz4_flex", label), |b| {
            b.iter(|| black_box(lz4_flex::block::decompress_into(black_box(&block), &mut flex)));
        });
    }
    group.finish();
}

#[cfg(not(feature = "liblz4"))]
fn same_bytes(_: &mut Criterion) {}

criterion_group!(benches, packing, packing_asm, sizes, unpacking, ratio, against_lz4, same_bytes);
criterion_main!(benches);
