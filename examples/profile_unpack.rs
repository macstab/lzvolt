//! A tight loop over `unpack`, for the profiler.
//!
//! Deliberately does nothing else: the varied 64 KB case is the one that lags
//! LZ4, and five attempts to close it by reasoning have returned between -4.5%
//! and +31%. This exists so the sixth is aimed.

use keva_core::store::pack;

/// Records-shaped, which is what reaches the wide split and where the day's
/// format work landed. Pass "varied" on the command line for the other one.
fn noise(total: usize) -> Vec<u8> {
    let mut st = 0x2545_F491_4F6C_DD1Du64;
    (0..total).map(|_| { st ^= st << 13; st ^= st >> 7; st ^= st << 17; st as u8 }).collect()
}

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
        "lz4_noise_4k" => noise(4096),
        "lz4_noise_64k" => noise(65_536),
        "lz4_records_4k" => records(4096),
        "lz4_records_64k" => records(65_536),
        "lz4_varied_4k" => varied(4096),
        "lz4_varied_512" => varied(512),
        // The two cells a Xeon loses by 29% on foreign blocks, which is the
        // whole reason this example exists on x86.
        "lz4_noise_512" => noise(512),
        "lz4_records_512" => records(512),
        "lz4_varied_64k" => varied(65_536),
        "varied_512" => varied(512),
        "records_512" => records(512),
        "records_192" => records(192),
        _ => records(4096),
    };
    eprintln!("Profil: {shape}, {} B", data.len());

    // The LZ4 interop body, which is a different kernel and the one behind the
    // noise gap. liblz4 writes the block, ours reads it -- the same thing the
    // same_bytes group measures.
    if shape.starts_with("lz4_") {
        #[link(name = "lz4")]
        extern "C" {
            fn LZ4_compress_default(s: *const u8, d: *mut u8, n: i32, cap: i32) -> i32;
        }
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
        let mut out = vec![0u8; data.len() + 64];
        let rounds: u64 = std::env::args().nth(2).and_then(|s| s.parse().ok()).unwrap_or(u64::MAX);
        let mut sink = 0u64;
        for _ in 0..rounds {
            assert!(keva_asm::unpack::unpack_lz4_into_slice(&block, &mut out, data.len()));
            sink = sink.wrapping_add(out[0] as u64);
        }
        println!("{sink}");
        return;
    }
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
