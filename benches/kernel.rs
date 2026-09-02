//! Hand-written search against the compiler's.
//!
//! Both are driven the way the packer drives them — repeatedly, carrying the
//! table and the skip counter across calls — so the number is what a real pack
//! would see rather than one call in isolation.
//!
//! The comparison this crate already made once came out against the assembly:
//! the control-group kernel loses to the intrinsics by 2.4x. Keeping both
//! compiled and measured is what makes either claim falsifiable.

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use keva_asm::pack_find::{self, PackState, SKIP_TRIGGER, TABLE_SIZE};

fn records(total: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(total);
    let mut i = 0u64;
    while out.len() < total {
        out.extend_from_slice(
            format!("{{\"id\":{i},\"tenant\":\"tenant42\",\"active\":true,\"role\":\"member\"}}")
                .as_bytes(),
        );
        i += 1;
    }
    out.truncate(total);
    out
}

fn varied(total: usize) -> Vec<u8> {
    let mut state = 0x2545_F491_4F6C_DD1Du64;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    let regions = ["eu-central-1", "us-east-1", "ap-south-1", "sa-east-1"];
    let mut out = Vec::with_capacity(total);
    while out.len() < total {
        let r = next();
        out.extend_from_slice(
            format!(
                "{{\"id\":\"{:016x}\",\"tenant\":\"{:08x}\",\"region\":\"{}\",\"score\":{}}}",
                r,
                next() as u32,
                regions[(r % 4) as usize],
                next() % 100_000
            )
            .as_bytes(),
        );
    }
    out.truncate(total);
    out
}

fn noise(total: usize) -> Vec<u8> {
    let mut state = 0x9E37_79B9_7F4A_7C15u64;
    (0..total)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state as u8
        })
        .collect()
}

/// One full pass, the way `pack` would make it.
fn sweep(input: &[u8], table: &mut [u32], asm: bool) -> u32 {
    let mut state = PackState {
        misses: 1 << SKIP_TRIGGER,
        ..Default::default()
    };
    let mut total = 0u32;
    loop {
        if asm {
            pack_find::find(input, table, &mut state);
        } else {
            pack_find::find_scalar(input, table, &mut state);
        }
        if state.len == 0 {
            return total;
        }
        total = total.wrapping_add(state.len);
        state.at += state.len;
    }
}

fn search(c: &mut Criterion) {
    let mut group = c.benchmark_group("pack_find");

    for (label, data) in [
        ("records_4k", records(4096)),
        ("records_64k", records(65_536)),
        ("varied_4k", varied(4096)),
        ("varied_64k", varied(65_536)),
        ("noise_4k", noise(4096)),
    ] {
        group.throughput(Throughput::Bytes(data.len() as u64));

        group.bench_function(BenchmarkId::new("asm", label), |b| {
            let mut table = vec![0u32; TABLE_SIZE];
            b.iter(|| black_box(sweep(black_box(&data), &mut table, true)));
        });

        group.bench_function(BenchmarkId::new("rust", label), |b| {
            let mut table = vec![0u32; TABLE_SIZE];
            b.iter(|| black_box(sweep(black_box(&data), &mut table, false)));
        });
    }

    group.finish();
}

criterion_group!(benches, search);
criterion_main!(benches);
