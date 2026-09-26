//! Every block shape liblz4 can emit, through the assembly LZ4 body.
//!
//! The paths that matter here are the rare ones: a match of 270 bytes or more,
//! which is where the extension byte is 255 and the length chain continues, and
//! a literal run long enough to leave the zone. Both are a handful of blocks in
//! a normal value and the whole of the risk.

fn main() {
    let mut bad = 0usize;
    let mut cases = 0usize;

    for seed in 0..64u64 {
        for &len in &[
            1usize, 2, 15, 16, 17, 31, 32, 63, 64, 127, 269, 270, 271, 512, 1000, 4096, 9000,
            65_536,
        ] {
            for kind in 0..5 {
                let data = make(kind, len, seed);
                let block = lz4_block(&data);
                let mut out = vec![0u8; data.len() + 64];
                let n = out.len();
                // Every body, not the one this machine happens to select.
                // A kernel built for a part line is a second program, and the
                // machine running the test is never all of them: on an Apple
                // host the x86 dispatch picks the baseline body, so the Xeon
                // and EPYC ones -- which is where the AVX literal loop lives --
                // went unexercised until a deliberate corruption in that loop
                // failed to turn this red.
                let ok = lzvolt::raw::Lz4Body::all().iter().all(|&body| {
                    lzvolt::raw::unpack_lz4_into_slice_on(body, &block, &mut out, data.len())
                });
                cases += 1;
                if !ok {
                    // A refusal is allowed -- the caller falls back -- but it
                    // must not be silent, so count it separately.
                    println!("  REFUSED kind={kind} len={len} seed={seed} (cap {n})");
                    bad += 1;
                    continue;
                }
                if out[..data.len()] != data[..] {
                    let at = (0..data.len()).find(|&i| out[i] != data[i]).unwrap();
                    println!("  WRONG kind={kind} len={len} seed={seed} at byte {at}");
                    bad += 1;
                }
            }
        }
    }
    println!("{cases} Faelle, {bad} falsch");
    assert_eq!(bad, 0, "the LZ4 body disagrees with what liblz4 wrote");
}

fn make(kind: u32, len: usize, seed: u64) -> Vec<u8> {
    let mut st = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    let mut next = move || {
        st ^= st << 13;
        st ^= st >> 7;
        st ^= st << 17;
        st
    };
    match kind {
        // One enormous match: every extension byte the format has.
        0 => vec![0x41u8; len],
        // Long matches broken by single literals.
        1 => (0..len)
            .map(|i| if i % 400 == 0 { (i / 400) as u8 } else { 0x42 })
            .collect(),
        // Incompressible: one literal run over the whole value.
        2 => (0..len).map(|_| next() as u8).collect(),
        // Literal runs just past the nibble, matches just under it.
        3 => (0..len).map(|i| ((i % 23) + (i / 23) % 3) as u8).collect(),
        // Mixed, with a repeating prefix so offsets vary widely.
        _ => {
            let mut v = Vec::with_capacity(len);
            while v.len() < len {
                let r = next();
                if r % 3 == 0 {
                    v.extend_from_slice(b"tenant42/eu-central-1/member");
                } else {
                    v.extend((0..(r % 19) as usize).map(|_| next() as u8));
                }
            }
            v.truncate(len);
            v
        }
    }
}

fn lz4_block(data: &[u8]) -> Vec<u8> {
    unsafe {
        let cap = lz4_compress_bound(data.len() as i32);
        let mut out = vec![0u8; cap as usize];
        let n = lz4_compress_default(
            data.as_ptr() as *const i8,
            out.as_mut_ptr() as *mut i8,
            data.len() as i32,
            cap,
        );
        out.truncate(n as usize);
        out
    }
}

#[link(name = "lz4")]
extern "C" {
    #[link_name = "LZ4_compressBound"]
    fn lz4_compress_bound(size: i32) -> i32;
    #[link_name = "LZ4_compress_default"]
    fn lz4_compress_default(src: *const i8, dst: *mut i8, src_size: i32, dst_cap: i32) -> i32;
}
