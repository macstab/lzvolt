//! Was drei Literalbits im even-Split kosten und was das Repeat-Bit spart.
//!
//! Beides sind Zaehlungen, keine Messungen. Der Preis von 3/4/1 sind die
//! Bloecke, deren Literallauf sieben erreicht und deshalb eine Laengenkette
//! braucht, wo heute erst fuenfzehn eine ausloest. Der Gewinn sind die
//! Bloecke, deren Offset der vorige schon war.

use lzvolt::format;

fn records(total: usize) -> Vec<u8> {
    let (mut o, mut i) = (Vec::new(), 0u64);
    while o.len() < total {
        o.extend_from_slice(
            format!(
                "{{\"id\":{i},\"tenant\":\"tenant42\",\"active\":true,\"role\":\"member\",\
                 \"created\":\"2026-09-0{}T1{}:0{}:00Z\",\"score\":{},\"region\":\"eu-central-1\"}}",
                i % 9, i % 10, i % 10, i % 1000
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

fn main() {
    println!(
        "{:13}{:>7}{:>8}{:>9}{:>9}{:>10}{:>10}",
        "", "Bloecke", "lit>=15", "lit>=7", "neu", "Repeats", "Bilanz"
    );
    for (name, data) in [
        ("records_192", records(192)),
        ("records_512", records(512)),
        ("varied_512", varied(512)),
        ("varied_4k", varied(4096)),
        ("varied_64k", varied(65_536)),
    ] {
        let mut packed = Vec::new();
        if !format::pack(&data, &mut packed) {
            println!("  {name}: roh gespeichert");
            continue;
        }
        // Der Kopf: zwei Bits Klasse, dann die Laenge. Danach beginnen die Bloecke.
        let klass = (packed[0] >> 6) as usize + 1;
        let hybrid = klass > 1 && packed[0] & 0x20 != 0;
        if hybrid {
            println!("  {name}: wechselt den Split, nicht rein even -- ausgelassen");
            continue;
        }
        let body = &packed[klass..];

        let (mut at, mut blocks) = (0usize, 0usize);
        let (mut chain_now, mut chain_then, mut repeats) = (0usize, 0usize, 0usize);
        let mut last_offset = usize::MAX;
        while at < body.len() {
            let token = body[at];
            at += 1;
            let mut lit = (token >> 4) as usize;
            if lit == 15 {
                chain_now += 1;
                loop {
                    let b = body[at];
                    at += 1;
                    lit += b as usize;
                    if b != 255 { break; }
                }
            }
            if lit >= 7 { chain_then += 1; }
            at += lit;
            blocks += 1;
            if at + 1 >= body.len() { break; }
            let offset = u16::from_le_bytes([body[at], body[at + 1]]) as usize;
            at += 2;
            if offset == last_offset { repeats += 1; }
            last_offset = offset;
            let mut mat = (token & 0x0F) as usize;
            if mat == 15 {
                loop {
                    let b = body[at];
                    at += 1;
                    mat += b as usize;
                    if b != 255 { break; }
                }
            }
        }
        // 3/4/1: jeder Repeat spart zwei Offsetbytes, jede neue Kette kostet eines.
        let saved = repeats * 2;
        let cost = chain_then.saturating_sub(chain_now);
        println!(
            "  {name:11}{blocks:7}{chain_now:8}{chain_then:9}{:9}{repeats:10}{:>10}",
            cost,
            format!("{:+} B", cost as isize - saved as isize)
        );
        println!(
            "               gepackt {} B -> {} B   ({:+.1}%)",
            packed.len(),
            packed.len() + cost - saved,
            100.0 * ((packed.len() + cost - saved) as f64 / packed.len() as f64 - 1.0)
        );
    }
}
