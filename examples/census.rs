//! What the decoder is asked, block by block, on a foreign LZ4 block.
//!
//! Throughput on a shape that lags is made of two things and they are not the
//! same: how many blocks there are per byte, and how many of the questions each
//! block asks are answered the same way as last time. A branch taken 50% of the
//! time costs nothing if it alternates in long runs and costs a mispredict
//! every other block if it does not, so the switch rate is counted here next to
//! the rate itself.
//!
//!   RUSTFLAGS="-L/opt/homebrew/lib" cargo run --release -p keva-core \
//!     --features liblz4 --example lz4_census

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
                "{{\"id\":{},\"tenant\":\"tenant{}\",\"active\":{},\"role\":\"{}\",\
                 \"created\":\"2026-09-{:02}T{:02}:{:02}:00Z\",\"score\":{},\"region\":\"{}\"}}",
                r % 1_000_000,
                r % 97,
                r % 2 == 0,
                roles[(r % 5) as usize],
                r % 28 + 1,
                r % 24,
                r % 60,
                r % 1000,
                regions[(r % 4) as usize],
            )
            .as_bytes(),
        );
    }
    out.truncate(total);
    out
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

/// A question the loop asks every block, with how often it is yes and how often
/// the answer differs from the block before.
#[derive(Default)]
struct Question {
    yes: usize,
    switches: usize,
    last: Option<bool>,
}

impl Question {
    fn see(&mut self, v: bool) {
        if v {
            self.yes += 1;
        }
        if self.last.is_some_and(|l| l != v) {
            self.switches += 1;
        }
        self.last = Some(v);
    }
}

fn main() {
    for (name, data) in [
        ("varied_512", varied(512)),
        ("varied_4k", varied(4096)),
        ("records_4k", records(4096)),
        ("records_64k", records(65_536)),
    ] {
        let block = lz4_block(&data);

        let (mut blocks, mut at) = (0usize, 0usize);
        let (mut lit_total, mut mat_total) = (0usize, 0usize);
        let mut lit_sat = Question::default();
        let mut mat_sat = Question::default();
        let mut near = Question::default();
        let mut lit_len_hist = [0usize; 6];
        let mut mat_len_hist = [0usize; 6];

        while at < block.len() {
            let token = block[at];
            at += 1;
            let mut lit = (token >> 4) as usize;
            lit_sat.see(lit == 15);
            if lit == 15 {
                loop {
                    let b = block[at];
                    at += 1;
                    lit += b as usize;
                    if b != 255 {
                        break;
                    }
                }
            }
            at += lit;
            lit_total += lit;
            lit_len_hist[bucket(lit)] += 1;

            if at >= block.len() {
                blocks += 1;
                break; // the last run carries no match
            }

            let offset = u16::from_le_bytes([block[at], block[at + 1]]) as usize;
            at += 2;
            let mut mat = (token & 0x0F) as usize;
            mat_sat.see(mat == 15);
            if mat == 15 {
                loop {
                    let b = block[at];
                    at += 1;
                    mat += b as usize;
                    if b != 255 {
                        break;
                    }
                }
            }
            mat += 4;
            mat_total += mat;
            mat_len_hist[bucket(mat)] += 1;
            near.see(offset < mat || offset < 32);
            blocks += 1;
        }

        println!("\n=== {name}   {} B -> {} B  ({:.2}x)", data.len(), block.len(),
                 data.len() as f64 / block.len() as f64);
        println!("   {blocks} Bloecke, {:.1} Bytes je Block  (Literale {:.1} + Match {:.1})",
                 data.len() as f64 / blocks as f64,
                 lit_total as f64 / blocks as f64,
                 mat_total as f64 / blocks as f64);
        for (label, q) in [
            ("Literalnibble gesaettigt (-> fast_litlong)", &lit_sat),
            ("Matchnibble gesaettigt   (-> fast_long)", &mat_sat),
            ("Offset kurz              (-> fast_near)", &near),
        ] {
            let n = blocks.max(1);
            println!("   {label:42} {:5.1}% ja, {:5.1}% Wechsel",
                     100.0 * q.yes as f64 / n as f64,
                     100.0 * q.switches as f64 / n as f64);
        }
        println!("   Literallaengen {}", hist(&lit_len_hist));
        println!("   Matchlaengen   {}", hist(&mat_len_hist));
    }
}

fn bucket(n: usize) -> usize {
    match n {
        0 => 0,
        1..=3 => 1,
        4..=14 => 2,
        15..=31 => 3,
        32..=63 => 4,
        _ => 5,
    }
}

fn hist(h: &[usize; 6]) -> String {
    let t: usize = h.iter().sum::<usize>().max(1);
    ["0", "1-3", "4-14", "15-31", "32-63", "64+"]
        .iter()
        .zip(h)
        .map(|(l, n)| format!("{l}:{:.0}%", 100.0 * *n as f64 / t as f64))
        .collect::<Vec<_>>()
        .join("  ")
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
