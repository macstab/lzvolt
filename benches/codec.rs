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

fn unpacking(c: &mut Criterion) {
    let mut group = c.benchmark_group("pack/decompress");

    for (label, data) in [
        ("records_512", records(512)),
        ("records_4k", records(4096)),
        ("records_64k", records(65_536)),
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
    for (label, data) in [("records_4k", records(4096)), ("noise_4k", noise(4096))] {
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

criterion_group!(benches, packing, unpacking, ratio);
criterion_main!(benches);
