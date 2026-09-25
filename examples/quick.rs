//! Thumbs up or thumbs down, in a few seconds.
//!
//! The packer report answers "what are the numbers" and takes forty minutes a
//! revision. That is the wrong instrument for the question this project asks a
//! hundred times a day, which is "did that change help". This one answers only
//! that, and it answers it before you have finished reading the command line.
//!
//! It is also a *better* instrument for a hand-written kernel than a benchmark
//! framework is. There is no harness between the clock and the call: the loop
//! is a loop, the timer is read twice, and the quantity reported is nanoseconds
//! per call -- which is what the cost model is written in, rather than GiB/s
//! medians that have to be converted back.
//!
//!   cargo run --release -p keva-core --features liblz4 --example quick
//!   cargo run --release -p keva-core --features liblz4 --example quick -- --save
//!
//! `--save` records the current numbers as the baseline. Every later run prints
//! the change against it, so the answer is an arrow rather than a table to
//! interpret. Make a change, run it, read the arrows.
//!
//! liblz4 is measured in the same loop on the same bytes, and its column is the
//! error bar: when it moves, the machine moved, and our column has to be read
//! against it rather than against the baseline.

use keva_core::store::pack;
use std::time::Instant;

const BASELINE: &str = "bench-results/quick.txt";

/// Milliseconds a cell is allowed. Fifteen cells and two implementations at
/// 120 ms is under four seconds of measuring; the rest of the runtime is
/// building the corpora, which is where this spends most of its life.
const CELL_MS: u128 = 300;

fn main() {
    let save = std::env::args().any(|a| a == "--save");
    let shapes = corpora();

    let old = load(BASELINE);
    let mut now: Vec<(String, f64, f64)> = Vec::new();

    println!(
        "{:<22}{:>10}{:>11}{:>12}{:>10}",
        "", "ns/Aufruf", "GB/s", "vs liblz4", "vs Basis"
    );

    for (name, data) in &shapes {
        // Foreign blocks: liblz4 writes, we read. The interop body.
        if let Some((block, len)) = lz4_block(data) {
            let mut mine = vec![0u8; len + 64];
            let ours = time(CELL_MS, || {
                assert!(keva_asm::unpack::unpack_lz4_into_slice(&block, &mut mine, len));
            });
            let theirs = time(CELL_MS, || {
                lz4_decompress(&block, &mut mine, len);
            });
            report(&format!("lz4/{name}"), len, ours, theirs, &old, &mut now);
        }

        // Our own format, where the packer accepts the value at all.
        let mut packed = Vec::new();
        if pack::pack(data, &mut packed) {
            let mut out = Vec::with_capacity(data.len() + 64);
            let ours = time(CELL_MS, || {
                pack::unpack(&packed, &mut out).unwrap();
            });
            report(&format!("own/{name}"), data.len(), ours, f64::NAN, &old, &mut now);
        }
    }

    if save {
        store(BASELINE, &now);
        println!("\nBasis geschrieben nach {BASELINE} ({} Zellen)", now.len());
    } else if old.is_empty() {
        println!("\nKeine Basis vorhanden. `--save` legt eine an.");
    } else {
        verdict(&old, &now);
    }
}

/// Nanoseconds per call, best of five, each measured over as many rounds as
/// fit in `budget_ms`.
///
/// Best of five and not the mean: a slow round is something else on the machine
/// -- another process, a frequency step, the scheduler moving the thread to a
/// small core -- and averaging it in reports that as the code's cost. A fast
/// round cannot be an artefact in the same way; nothing makes a loop finish
/// work it did not do.
///
/// Five rather than three because the small cells need it. A 512-byte
/// incompressible value decodes in about nine nanoseconds, and on a laptop the
/// same code measured 9.2 and 18.8 in two consecutive runs at three.
fn time(budget_ms: u128, mut f: impl FnMut()) -> f64 {
    for _ in 0..8 {
        f();
    }
    let mut best = f64::MAX;
    for _ in 0..5 {
        let mut rounds: u64 = 1;
        loop {
            let t = Instant::now();
            for _ in 0..rounds {
                f();
            }
            let ns = t.elapsed().as_nanos();
            if ns >= budget_ms * 1_000_000 / 5 {
                best = best.min(ns as f64 / rounds as f64);
                break;
            }
            rounds = rounds.saturating_mul(4);
        }
    }
    best
}

fn report(
    name: &str,
    bytes: usize,
    ours: f64,
    theirs: f64,
    old: &[(String, f64, f64)],
    now: &mut Vec<(String, f64, f64)>,
) {
    let gbs = bytes as f64 / ours;
    let vs_lib = if theirs.is_nan() {
        String::from("--")
    } else {
        format!("{:+.1}%", 100.0 * (theirs / ours - 1.0))
    };
    let vs_base = match old.iter().find(|(n, _, _)| n == name) {
        // Less time is better, so the sign is flipped to read as "faster".
        Some((_, prev, _)) => format!("{:+.1}%", 100.0 * (prev / ours - 1.0)),
        None => String::from("neu"),
    };
    println!("{name:<22}{ours:>10.1}{gbs:>11.2}{vs_lib:>12}{vs_base:>10}");
    now.push((name.to_string(), ours, theirs));
}

/// One line per cell, because a format nobody has to parse cannot drift.
fn store(path: &str, cells: &[(String, f64, f64)]) {
    let _ = std::fs::create_dir_all(std::path::Path::new(path).parent().unwrap());
    let body: String = cells
        .iter()
        .map(|(n, a, b)| format!("{n}\t{a}\t{b}\n"))
        .collect();
    let _ = std::fs::write(path, body);
}

fn load(path: &str) -> Vec<(String, f64, f64)> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    text.lines()
        .filter_map(|l| {
            let mut f = l.split('\t');
            Some((
                f.next()?.to_string(),
                f.next()?.parse().ok()?,
                f.next()?.parse().unwrap_or(f64::NAN),
            ))
        })
        .collect()
}

/// The arrow.
///
/// A cell counts as moved only past three percent, because that is roughly
/// where this harness stops being able to tell code from machine -- and the
/// liblz4 column is checked first, since a run where the control moved is a run
/// about the machine and not about the change.
fn verdict(old: &[(String, f64, f64)], now: &[(String, f64, f64)]) {
    let (mut up, mut down, mut muted) = (Vec::new(), Vec::new(), Vec::new());
    for (name, ns, lib) in now {
        let Some((_, prev, prev_lib)) = old.iter().find(|(n, _, _)| n == name) else {
            continue;
        };
        let d = 100.0 * (prev / ns - 1.0);
        // Per cell and not per run: varied_64k's control wanders several
        // percent on a laptop, and letting that suppress the verdict on
        // noise_4k throws away the answer because a different question was
        // noisy.
        let drift = if lib.is_nan() || prev_lib.is_nan() {
            0.0
        } else {
            (prev_lib / lib - 1.0).abs() * 100.0
        };
        if drift > 3.0 {
            if d.abs() >= 3.0 {
                muted.push((name.clone(), d, drift));
            }
            continue;
        }
        if d >= 3.0 {
            up.push((name.clone(), d));
        } else if d <= -3.0 {
            down.push((name.clone(), d));
        }
    }
    println!();
    match (up.is_empty(), down.is_empty()) {
        (true, true) => println!("=  unveraendert"),
        (false, true) => {
            println!("^  BESSER");
            for (n, d) in &up {
                println!("     {n:<22}{d:+.1}%");
            }
        }
        (true, false) => {
            println!("v  SCHLECHTER");
            for (n, d) in &down {
                println!("     {n:<22}{d:+.1}%");
            }
        }
        (false, false) => {
            println!("~  gemischt");
            for (n, d) in up.iter().chain(down.iter()) {
                println!("     {n:<22}{d:+.1}%");
            }
        }
    }
    for (n, d, drift) in &muted {
        println!("?  {n:<22}{d:+.1}%  -- verworfen, liblz4 selbst {drift:.1}%");
    }
}

// ---- the corpora, the same three shapes the rest of the project uses --------

fn corpora() -> Vec<(String, Vec<u8>)> {
    let mut v = Vec::new();
    for (kind, f) in [
        ("records", records as fn(usize) -> Vec<u8>),
        ("varied", varied),
        ("noise", noise),
    ] {
        for (label, n) in [("512", 512usize), ("4k", 4096), ("64k", 65_536)] {
            v.push((format!("{kind}_{label}"), f(n)));
        }
    }
    v
}

fn noise(total: usize) -> Vec<u8> {
    let mut st = 0x2545_F491_4F6C_DD1Du64;
    (0..total)
        .map(|_| {
            st ^= st << 13;
            st ^= st >> 7;
            st ^= st << 17;
            st as u8
        })
        .collect()
}

fn records(total: usize) -> Vec<u8> {
    let (mut o, mut i) = (Vec::new(), 0u64);
    while o.len() < total {
        o.extend_from_slice(
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
    o.truncate(total);
    o
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

// ---- liblz4, when it is linked ---------------------------------------------

#[cfg(feature = "liblz4")]
#[link(name = "lz4")]
extern "C" {
    fn LZ4_compress_default(s: *const u8, d: *mut u8, n: i32, cap: i32) -> i32;
    fn LZ4_decompress_safe(s: *const u8, d: *mut u8, n: i32, cap: i32) -> i32;
}

#[cfg(feature = "liblz4")]
fn lz4_block(data: &[u8]) -> Option<(Vec<u8>, usize)> {
    let mut block = vec![0u8; data.len() * 2 + 64];
    let n = unsafe {
        LZ4_compress_default(
            data.as_ptr(),
            block.as_mut_ptr(),
            data.len() as i32,
            block.len() as i32,
        )
    };
    block.truncate(n as usize);
    Some((block, data.len()))
}

#[cfg(feature = "liblz4")]
fn lz4_decompress(block: &[u8], out: &mut [u8], len: usize) {
    unsafe {
        LZ4_decompress_safe(
            block.as_ptr(),
            out.as_mut_ptr(),
            block.len() as i32,
            len as i32,
        );
    }
}

#[cfg(not(feature = "liblz4"))]
fn lz4_block(_: &[u8]) -> Option<(Vec<u8>, usize)> {
    None
}

#[cfg(not(feature = "liblz4"))]
fn lz4_decompress(_: &[u8], _: &mut [u8], _: usize) {}
