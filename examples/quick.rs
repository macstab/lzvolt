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
//!   cargo run --release --features liblz4 --example quick
//!   cargo run --release --features liblz4 --example quick -- --save
//!
//! `--save` records the current numbers as the baseline. Every later run prints
//! the change against it, so the answer is an arrow rather than a table to
//! interpret. Make a change, run it, read the arrows.
//!
//! liblz4 is measured in the same loop on the same bytes, and its column is the
//! error bar: when it moves, the machine moved, and our column has to be read
//! against it rather than against the baseline.

use lzvolt::format;
use std::time::Instant;

const BASELINE: &str = "bench-results/quick.txt";

/// Milliseconds a cell is allowed. Fifteen cells and two implementations at
/// 120 ms is under four seconds of measuring; the rest of the runtime is
/// building the corpora, which is where this spends most of its life.
const CELL_MS: u128 = 300;

fn main() {
    if std::env::args().any(|a| a == "--entry") {
        entry_cost();
        return;
    }
    let save = std::env::args().any(|a| a == "--save");
    let shapes = corpora();

    let old = load(BASELINE);
    let mut now: Vec<Cell> = Vec::new();

    // Which machine produced these. A table of nanoseconds says nothing
    // without it, and a table pasted into a log without it says less.
    println!(
        "{}  |  {}  |  kernel: {}",
        machine(),
        std::env::consts::ARCH,
        lzvolt::backend()
    );
    println!(
        "{:<22}{:>10}{:>11}{:>12}{:>10}{:>9}",
        "", "ns/call", "GB/s", "vs liblz4", "vs base", "res."
    );

    for (name, data) in &shapes {
        // The reference for both rows of a shape, measured in this process.
        let mut reference = f64::NAN;

        // Foreign blocks: liblz4 writes, we read. The interop body.
        if let Some((raw, len)) = lz4_block(data) {
            let mut ar = Arena::new(raw.len(), len + 64);
            ar.buf[ar.src..ar.src + raw.len()].copy_from_slice(&raw);
            // Split the arena once so the two closures can hold disjoint
            // halves; the offsets are what make the layout reproducible.
            let (head, tail) = ar.buf.split_at_mut(ar.a);
            let block: &[u8] = &head[ar.src..ar.src + raw.len()];
            let (dst_a, dst_b) = tail.split_at_mut(ar.b - ar.a);
            let (ours, theirs, spread) = time_pair(
                CELL_MS,
                || {
                    assert!(lzvolt::raw::unpack_lz4_into_slice(
                        block,
                        &mut dst_a[..len + 64],
                        len
                    ));
                },
                || lz4_decompress(block, &mut dst_b[..len + 64], len),
            );
            reference = theirs;
            report(
                &format!("lz4/{name}"),
                len,
                ours,
                theirs,
                spread,
                &old,
                &mut now,
            );
        }

        // Our own format, where the packer accepts the value at all.
        let mut packed = Vec::new();
        if format::pack(data, &mut packed) {
            let mut out = pinned_vec(data.len() + 4160);
            let (ours, spread) = time(CELL_MS, || {
                format::unpack(&packed, &mut out).unwrap();
            });
            report(
                &format!("own/{name}"),
                data.len(),
                ours,
                reference,
                spread,
                &old,
                &mut now,
            );
        }
    }

    if save {
        store(BASELINE, &now);
        println!("\nBaseline written to {BASELINE} ({} cells)", now.len());
    } else if old.is_empty() {
        println!("\nNo baseline yet. `--save` writes one.");
    } else {
        verdict(&old, &now);
    }
}

/// Source and destinations carved out of one allocation at fixed offsets.
///
/// Aligning each buffer to 64 bytes was not enough. What also matters is the
/// distance between them modulo 4096: when a load and a recent store land on
/// the same 4 KiB offset the hardware has to disambiguate them, and that is
/// worth tens of percent on a small copy. Separate `Vec`s put that distance
/// wherever the allocator felt like, which is stable inside one process and
/// different in the next -- so the same binary measured `lz4/varied_512` 32%
/// apart between two runs, five times in a row, with nothing changed.
///
/// One arena, fixed offsets, and a deliberate 1088-byte stagger so the three
/// regions never share a 4 KiB offset. The number this produces is not the
/// production number -- production gets whatever the allocator gives it -- but
/// it is the same number every run, which is the only property a comparison
/// needs.
struct Arena {
    buf: Vec<u8>,
    src: usize,
    a: usize,
    b: usize,
}

impl Arena {
    fn new(src_len: usize, dst_len: usize) -> Self {
        let stride = (src_len.max(dst_len) + 4096 + 63) & !63;
        let mut buf = vec![0u8; stride * 3 + 4096 + 128];
        let base = buf.as_ptr().align_offset(4096);
        let _ = &mut buf;
        Self {
            buf,
            src: base,
            a: base + stride + 1088,
            b: base + stride * 2 + 2176,
        }
    }
}

/// Time two implementations against each other, interleaved.
///
/// Not one after the other. Measuring ours for three hundred milliseconds and
/// then theirs for three hundred puts a frequency step between the two numbers,
/// and the ratio then describes the step. Interleaved, both halves of every
/// repetition see the same machine, and the ratio survives whatever the clock
/// was doing.
///
/// The answer is the *median* of five ratios rather than the best of five. Best
/// is right for a single measurement -- nothing makes a loop finish work it did
/// not do -- but a ratio has a fast side and a slow side and the best of it is
/// simply the luckiest pairing.
fn time_pair(budget_ms: u128, mut a: impl FnMut(), mut b: impl FnMut()) -> (f64, f64, f64) {
    for _ in 0..8 {
        a();
        b();
    }
    // One round count for both, so the two halves do the same amount of work --
    // and computed rather than reached by doubling.
    //
    // It used to quadruple until the budget was met, which quantises the round
    // count to a power of four. A cell near a boundary lands on one side in one
    // run and the other side in the next, and four times the loop length is
    // four times the cache and TLB pressure. That is constant inside a run and
    // different between runs, which is exactly the shape of the phantom
    // regressions this harness kept reporting: own/varied_512 at -31% with
    // nothing changed, five runs agreeing with each other and none with the
    // baseline.
    let probe = Instant::now();
    for _ in 0..64 {
        a();
    }
    let per_call = (probe.elapsed().as_nanos() as f64 / 64.0).max(0.25);
    let rounds = ((budget_ms as f64 * 1_000_000.0 / 5.0) / per_call).max(1.0) as u64;

    let mut pairs: Vec<(f64, f64)> = Vec::with_capacity(5);
    for _ in 0..5 {
        let t = Instant::now();
        for _ in 0..rounds {
            a();
        }
        let ta = t.elapsed().as_nanos() as f64 / rounds as f64;
        let t = Instant::now();
        for _ in 0..rounds {
            b();
        }
        let tb = t.elapsed().as_nanos() as f64 / rounds as f64;
        pairs.push((ta, tb));
    }
    pairs.sort_by(|x, y| (x.0 / x.1).partial_cmp(&(y.0 / y.1)).unwrap());
    // The middle ratio, and how far the neighbours sit from it. That spread is
    // this cell's resolution: a nine-nanosecond call on a laptop cannot be read
    // to three percent, and a fixed threshold either cries wolf there or goes
    // deaf on the large cells. Measuring it costs nothing -- the five samples
    // are already in hand.
    let lo = pairs[1].0 / pairs[1].1;
    let hi = pairs[3].0 / pairs[3].1;
    let spread = 100.0 * (hi / lo - 1.0);
    (pairs[2].0, pairs[2].1, spread)
}

/// What a call costs before it has copied anything, split by layer.
///
/// `t = F + n/B` over two sizes gives the fixed cost without needing the clock
/// rate. Doing it twice -- once through `unpack_lz4_into_slice`, which asks
/// which body to run on every call, and once through the `_on` form with that
/// answer hoisted out of the loop -- splits the fixed cost into the Rust
/// dispatch and everything else.
#[cfg(feature = "liblz4")]
fn entry_cost() {
    let body = lzvolt::raw::lz4_body();
    println!("Body: {body:?}\n");
    let mut rows: Vec<(&str, f64, f64)> = Vec::new();
    for (label, n) in [("512", 512usize), ("4096", 4096)] {
        let data = noise(n);
        let (raw, len) = lz4_block(&data).unwrap();
        let mut ar = Arena::new(raw.len(), len + 64);
        ar.buf[ar.src..ar.src + raw.len()].copy_from_slice(&raw);
        let (head, tail) = ar.buf.split_at_mut(ar.a);
        let block: &[u8] = &head[ar.src..ar.src + raw.len()];
        let (dst, _) = tail.split_at_mut(ar.b - ar.a);
        // Same buffer for both so the only difference is the dispatch.
        let with = {
            let d = &mut *dst;
            time(400, || {
                assert!(lzvolt::raw::unpack_lz4_into_slice(block, d, len));
            })
            .0
        };
        let without = {
            let d = &mut *dst;
            time(400, || {
                assert!(lzvolt::raw::unpack_lz4_into_slice_on(body, block, d, len));
            })
            .0
        };
        rows.push((label, with, without));
        println!("{label:>6} B   with dispatch {with:>8.2} ns   without {without:>8.2} ns");
    }
    let fit = |a: f64, b: f64| {
        let bw = 3584.0 / (b - a);
        (a - 512.0 / bw, bw)
    };
    let (fw, bw) = fit(rows[0].1, rows[1].1);
    let (fo, bo) = fit(rows[0].2, rows[1].2);
    println!("\n{:<24}{:>12}{:>14}", "", "fixed cost", "rate");
    println!("{:<24}{fw:>9.2} ns{bw:>11.1} GB/s", "with dispatch");
    println!("{:<24}{fo:>9.2} ns{bo:>11.1} GB/s", "without dispatch");
    println!("{:<24}{:>9.2} ns", "davon Rust-Dispatch", fw - fo);
}

#[cfg(not(feature = "liblz4"))]
fn entry_cost() {
    println!("braucht --features liblz4");
}

/// A `Vec` whose buffer starts at a known offset modulo 4096.
///
/// The LZ4 cells decode into an arena at fixed offsets; the own-format path
/// cannot, because `format::unpack` takes a `&mut Vec<u8>` and a `Vec` owns its
/// allocation. Address-space randomisation then puts that allocation at a
/// different 4 KiB offset every process, and `own/varied_512` read 30% apart
/// between two runs of the same binary because of it.
///
/// So: allocate a handful of candidates, keep the one that lands where we want,
/// drop the rest. Crude, and it settles the layout -- which is all a comparison
/// needs. Duplicating the length-header parsing here to reach the slice API
/// would be worse: that logic can drift from the format and this file would not
/// notice.
fn pinned_vec(cap: usize) -> Vec<u8> {
    let mut candidates: Vec<Vec<u8>> = Vec::new();
    for _ in 0..48 {
        let v: Vec<u8> = Vec::with_capacity(cap);
        if v.as_ptr() as usize % 4096 == 1088 {
            return v;
        }
        candidates.push(v);
    }
    // Nothing landed on the mark in forty-eight tries. Take the first one --
    // that cell is then as noisy as it was before, and its own measured spread
    // will say so rather than the verdict inventing a finding.
    candidates.swap_remove(0)
}

/// One implementation, with the spread taken from its own samples.
///
/// Not `time_pair` against an empty closure: that divides by a time near zero,
/// and the spread came out `inf` -- which made the verdict threshold infinite
/// and meant a regression in our *own* format could never be reported. Those
/// are the cells that are not allowed to move at all.
fn time(budget_ms: u128, mut f: impl FnMut()) -> (f64, f64) {
    for _ in 0..8 {
        f();
    }
    let probe = Instant::now();
    for _ in 0..64 {
        f();
    }
    let per_call = (probe.elapsed().as_nanos() as f64 / 64.0).max(0.25);
    let rounds = ((budget_ms as f64 * 1_000_000.0 / 5.0) / per_call).max(1.0) as u64;

    let mut v: Vec<f64> = Vec::with_capacity(5);
    for _ in 0..5 {
        let t = Instant::now();
        for _ in 0..rounds {
            f();
        }
        v.push(t.elapsed().as_nanos() as f64 / rounds as f64);
    }
    v.sort_by(|x, y| x.partial_cmp(y).unwrap());
    (v[2], 100.0 * (v[3] / v[1] - 1.0))
}

type Cell = (String, f64, f64, f64);

fn report(
    name: &str,
    bytes: usize,
    ours: f64,
    theirs: f64,
    spread: f64,
    old: &[Cell],
    now: &mut Vec<Cell>,
) {
    let gbs = bytes as f64 / ours;
    let vs_lib = if theirs.is_nan() {
        String::from("--")
    } else {
        format!("{:+.1}%", 100.0 * (theirs / ours - 1.0))
    };
    // Against the baseline as a *ratio* to liblz4, not as absolute time.
    //
    // Absolute nanoseconds are not comparable between two invocations on a
    // laptop: frequency and core assignment move them five to eight percent
    // with the code untouched, which this harness duly reported as "better"
    // until the reference went in. liblz4 runs in the same process on the same
    // bytes, so dividing by it cancels whatever the machine was doing.
    let vs_base = match old.iter().find(|(n, _, _, _)| n == name) {
        Some((_, prev, prev_ref, _)) if !theirs.is_nan() && !prev_ref.is_nan() => {
            let was = prev / prev_ref;
            let is = ours / theirs;
            format!("{:+.1}%", 100.0 * (was / is - 1.0))
        }
        Some(_) => String::from("--"),
        None => String::from("neu"),
    };
    println!(
        "{name:<22}{ours:>10.1}{gbs:>11.2}{vs_lib:>12}{vs_base:>10}{:>9}",
        format!("+-{spread:.1}%")
    );
    now.push((name.to_string(), ours, theirs, spread));
}

/// One line per cell, because a format nobody has to parse cannot drift.
fn store(path: &str, cells: &[Cell]) {
    let _ = std::fs::create_dir_all(std::path::Path::new(path).parent().unwrap());
    use std::fmt::Write;
    let mut body = String::new();
    for (n, a, b, sp) in cells {
        let _ = writeln!(body, "{n}\t{a}\t{b}\t{sp}");
    }
    let _ = std::fs::write(path, body);
}

fn load(path: &str) -> Vec<Cell> {
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
                f.next().and_then(|v| v.parse().ok()).unwrap_or(6.0),
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
fn verdict(old: &[Cell], now: &[Cell]) {
    let (mut up, mut down, mut blind) = (Vec::new(), Vec::new(), 0usize);
    for (name, ns, reference, spread) in now {
        let Some((_, prev, prev_ref, prev_spread)) = old.iter().find(|(n, _, _, _)| n == name)
        else {
            continue;
        };
        if reference.is_nan() || prev_ref.is_nan() {
            blind += 1;
            continue;
        }
        let d = 100.0 * ((prev / prev_ref) / (ns / reference) - 1.0);
        // The cell's own resolution, from both runs, and never below six
        // percent. A move smaller than what the harness can see is not a
        // finding, and reporting it as one is how a morning gets spent chasing
        // a change that was the scheduler. Six and not three because six is
        // what this measured: with the layout pinned, unchanged code still
        // wandered up to 5.8% between runs. Nothing this project has actually
        // won was smaller than 8%, so the floor costs no real finding -- and
        // the number that goes in the log comes from rented hardware anyway.
        let limit = spread.max(*prev_spread).max(6.0);
        if d >= limit {
            up.push((name.clone(), d, limit));
        } else if d <= -limit {
            down.push((name.clone(), d, limit));
        }
    }
    println!();
    match (up.is_empty(), down.is_empty()) {
        (true, true) => println!("=  UNCHANGED"),
        (false, true) => {
            println!("^  BETTER");
            for (n, d, l) in &up {
                println!("     {n:<22}{d:+.1}%   (resolution {l:.1}%)");
            }
        }
        (true, false) => {
            println!("v  WORSE");
            for (n, d, l) in &down {
                println!("     {n:<22}{d:+.1}%   (resolution {l:.1}%)");
            }
        }
        (false, false) => {
            println!("~  MIXED");
            for (n, d, l) in up.iter().chain(down.iter()) {
                println!("     {n:<22}{d:+.1}%   (resolution {l:.1}%)");
            }
        }
    }
    if blind > 0 {
        println!("   ({blind} cells with no reference -- built without liblz4?)");
    }
}

/// The processor's own name for itself, for the banner above the table.
///
/// Not `std::env::consts::ARCH`, which would say `aarch64` for both an M2 and
/// a Neoverse V2 -- two machines whose numbers are not comparable and whose
/// kernels are not even the same code. Falls back to the architecture when the
/// platform will not say.
fn machine() -> String {
    #[cfg(target_os = "macos")]
    {
        if let Ok(out) = std::process::Command::new("sysctl")
            .args(["-n", "machdep.cpu.brand_string"])
            .output()
        {
            let name = String::from_utf8_lossy(&out.stdout).trim().to_owned();
            if !name.is_empty() {
                return name;
            }
        }
    }
    #[cfg(target_os = "linux")]
    {
        if let Ok(text) = std::fs::read_to_string("/proc/cpuinfo") {
            // x86 says "model name"; ARM parts usually do not, and say
            // "CPU implementer" and "CPU part" instead.
            for key in ["model name", "Model"] {
                if let Some(line) = text.lines().find(|l| l.starts_with(key)) {
                    if let Some((_, v)) = line.split_once(':') {
                        return v.trim().to_owned();
                    }
                }
            }
            let part = text
                .lines()
                .find(|l| l.starts_with("CPU part"))
                .and_then(|l| l.split_once(':'))
                .map(|(_, v)| v.trim().to_owned());
            if let Some(part) = part {
                // 0xd4f is Neoverse V2, which is the one this project tunes for.
                let named = if part == "0xd4f" {
                    " (Neoverse V2)"
                } else {
                    ""
                };
                return format!("ARM part {part}{named}");
            }
        }
    }
    std::env::consts::ARCH.to_owned()
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
