//! A tight loop over `unpack`, for the profiler.
//!
//! Deliberately does nothing else: the varied 64 KB case is the one that lags
//! LZ4, and five attempts to close it by reasoning have returned between -4.5%
//! and +31%. This exists so the sixth is aimed.

use keva_core::store::pack;

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
    let data = varied(65_536);
    let mut packed = Vec::new();
    assert!(pack::pack(&data, &mut packed));

    let mut out = Vec::with_capacity(data.len() + 64);
    let mut sink = 0u64;
    for _ in 0..200_000 {
        pack::unpack(&packed, &mut out).unwrap();
        sink = sink.wrapping_add(out[0] as u64);
    }
    println!("{sink}");
}
