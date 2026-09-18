//! A tight loop over `unpack`, for the profiler.
//!
//! Deliberately does nothing else: the varied 64 KB case is the one that lags
//! LZ4, and five attempts to close it by reasoning have returned between -4.5%
//! and +31%. This exists so the sixth is aimed.

use keva_core::store::pack;

/// Records-shaped, which is what reaches the wide split and where the day's
/// format work landed. Pass "varied" on the command line for the other one.
fn records(total: usize) -> Vec<u8> {
    let (mut o, mut i) = (Vec::new(), 0u64);
    while o.len() < total {
        o.extend_from_slice(
            format!("{{\"id\":{i},\"tenant\":\"tenant42\",\"active\":true,\"role\":\"member\",\
                     \"created\":\"2026-09-0{}T1{}:0{}:00Z\",\"score\":{},\"region\":\"eu-central-1\"}}",
                    i % 9, i % 10, i % 10, i % 1000)
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

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let shape = args.get(1).map(String::as_str).unwrap_or("records_4k");
    let data = match shape {
        "varied_64k" => varied(65_536),
        "varied_4k" => varied(4096),
        "records_64k" => records(65_536),
        _ => records(4096),
    };
    eprintln!("Profil: {shape}, {} B", data.len());
    let mut packed = Vec::new();
    assert!(pack::pack(&data, &mut packed));

    let mut out = Vec::with_capacity(data.len() + 64);
    let mut sink = 0u64;
    // Runs until killed: a fixed count finishes before a sampler can attach,
    // and the point here is to be sampled rather than to be timed.
    let rounds: u64 = std::env::args()
        .nth(2)
        .and_then(|s| s.parse().ok())
        .unwrap_or(u64::MAX);
    for _ in 0..rounds {
        pack::unpack(&packed, &mut out).unwrap();
        sink = sink.wrapping_add(out[0] as u64);
    }
    println!("{sink}");
}
