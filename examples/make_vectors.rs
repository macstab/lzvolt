//! Write `docs/vectors.txt`, the decoder test vectors that go with the spec.
//!
//! Run with `cargo run --example make_vectors`. Every vector is checked against
//! the decoder here before it is written, and `the_published_vectors_hold` in
//! `src/format.rs` checks the committed file again on every test run. This
//! program exists so the file can be regenerated, not so it can be trusted --
//! the test is what holds it.
//!
//! The vectors are **decoder** vectors: a stream and what it must produce, or a
//! stream and the fact that it must be refused. They are deliberately not
//! encoder vectors, because what an encoder emits for a given input is its own
//! choice (see docs/FORMAT.md, "What an encoder is free to choose") and pinning
//! it would make a conforming implementation look wrong.

use std::fmt::Write as _;

enum Expect {
    Output(Vec<u8>),
    /// A unit repeated, for outputs too large to spell out in hex.
    Repeat(Vec<u8>, usize),
    Reject(&'static str),
}

struct Vector {
    name: &'static str,
    note: &'static str,
    stream: Vec<u8>,
    expect: Expect,
}

fn main() {
    let mut v = Vec::new();

    // ---- The smallest things the format can say ------------------------

    // Header class 0, declared 0; one token with no literals and no match.
    v.push(Vector {
        name: "empty value",
        note: "Class 0 declaring zero. Decoding ends the moment it begins.",
        stream: vec![0x00, 0x00],
        expect: Expect::Output(vec![]),
    });

    v.push(Vector {
        name: "one literal",
        note: "The literal nibble carries the length; no match follows.",
        stream: vec![0x01, 0x10, b'a'],
        expect: Expect::Output(b"a".to_vec()),
    });

    // ---- Extension chains ----------------------------------------------

    // L saturates at 15, so a chain follows even when it adds nothing.
    let mut s = vec![15u8, 0xF0, 0x00];
    s.extend_from_slice(&[b'a'; 15]);
    v.push(Vector {
        name: "literal length exactly at the nibble maximum",
        note: "Fifteen saturates the field, so a chain byte of zero follows. \
               Fifteen is not encodable without it.",
        stream: s,
        expect: Expect::Output(vec![b'a'; 15]),
    });

    // 15 + 255 + 0: a chain that continues once.
    let mut s = vec![];
    put_header(270, false, &mut s);
    s.extend_from_slice(&[0xF0, 0xFF, 0x00]);
    s.extend_from_slice(&[b'b'; 270]);
    v.push(Vector {
        name: "literal chain of two bytes",
        note: "255 means another byte follows; the total is added to the field. \
               15 + 255 + 0 = 270.",
        stream: s,
        expect: Expect::Repeat(b"b".to_vec(), 270),
    });

    // ---- Matches --------------------------------------------------------

    // "abcd" then a match of 4 at offset 4. The stream ends on the match.
    let mut s = vec![8u8, 0x40];
    s.extend_from_slice(b"abcd");
    s.extend_from_slice(&4u16.to_le_bytes());
    v.push(Vector {
        name: "shortest possible match",
        note: "Match length is stored less four, so a nibble of zero is a match \
               of four. The stream ends here: declared is reached, and no \
               closing token is required.",
        stream: s,
        expect: Expect::Output(b"abcdabcd".to_vec()),
    });

    // One literal, then offset 1 for nine more -- a run, not a copy.
    v.push(Vector {
        name: "overlapping match, offset one",
        note: "Offset 1 with length 9 repeats a single byte. A decoder copying \
               this with a wide load would read bytes it has not written yet.",
        stream: vec![10, 0x15, b'a', 0x01, 0x00],
        expect: Expect::Output(vec![b'a'; 10]),
    });

    // Match length saturates the nibble and continues in a chain.
    let mut s = vec![];
    put_header(300, false, &mut s);
    // Four literals, then a match of 296 at offset 4: 296 - 4 = 292 = 15 + 277,
    // so the chain is 255 then 22.
    s.push(0x4F);
    s.extend_from_slice(b"abcd");
    s.extend_from_slice(&4u16.to_le_bytes());
    s.extend_from_slice(&[0xFF, 22]);
    v.push(Vector {
        name: "match chain of two bytes",
        note: "The match chain sits after the offset, and is read exactly as the \
               literal chain is.",
        stream: s,
        expect: Expect::Repeat(b"abcd".to_vec(), 75),
    });

    // ---- The hybrid switch, which LZ4 has no equivalent for -------------

    // Even section: 60 literals and a match of 4, reaching out_at = 64 on a
    // block that carries a match. That last part is not decoration -- a
    // section other than the last may not end on a literals-only block, and
    // the first draft of this vector did, which is how the rule was found.
    // See docs/FORMAT.md, "Across the boundary".
    let mut body = vec![0xF0, 60 - 15];
    body.extend_from_slice(&[b'c'; 60]);
    body.extend_from_slice(&1u16.to_le_bytes()); // offset 1: four more 'c'
    let in_at = body.len();
    // Wide token: LL MMMMM R. No literals, match length 36 -> 32 saturates the
    // five-bit field at 31, so a chain of 1 follows the offset.
    body.push(31 << 1); // L = 0, M = 31 (saturated), R = 0
    body.extend_from_slice(&64u16.to_le_bytes());
    body.push(1);
    let mut s = vec![];
    put_header(100, true, &mut s);
    s.extend_from_slice(&body);
    s.extend_from_slice(&(in_at as u16).to_le_bytes());
    s.extend_from_slice(&64u16.to_le_bytes());
    v.push(Vector {
        name: "hybrid stream, even then wide",
        note: "The hybrid flag is set, so a trailer follows the body: in_at then \
               out_at, two bytes each because declared is under 65536. The even \
               section ends on a block that carries a match, which it must. The \
               wide section's match then reaches back across the switch into \
               what the even section produced.",
        stream: s,
        expect: Expect::Repeat(b"c".to_vec(), 100),
    });

    // ---- Every header class ---------------------------------------------

    for (declared, label) in [
        (
            64usize,
            "class 1, the shortest length that needs two header bytes",
        ),
        (8192, "class 2, three header bytes"),
        (2_097_152, "class 3, four header bytes"),
    ] {
        let mut s = vec![];
        put_header(declared, false, &mut s);
        // One literal, then one long match at offset 1.
        s.push(0x1F);
        s.push(b'z');
        s.extend_from_slice(&1u16.to_le_bytes());
        let mut rest = declared - 1 - 4 - 15;
        while rest >= 255 {
            s.push(255);
            rest -= 255;
        }
        s.push(rest as u8);
        v.push(Vector {
            name: Box::leak(format!("header {label}").into_boxed_str()),
            note: "The class is the shortest that holds the length, which is the \
                   only encoding a decoder may accept.",
            stream: s,
            expect: Expect::Repeat(b"z".to_vec(), declared),
        });
    }

    // ---- What must be refused -------------------------------------------

    v.push(Vector {
        name: "offset of zero",
        note: "Zero is not a backward reference. It is also what a decoder's \
               remembered offset starts at, which is what refuses a repeat flag \
               on a section's first block.",
        stream: vec![8, 0x10, b'a', 0x00, 0x00],
        expect: Expect::Reject("BadOffset"),
    });

    v.push(Vector {
        name: "offset reaching before the start",
        note: "One byte has been produced, so an offset of 99 points outside the \
               output entirely.",
        stream: vec![8, 0x10, b'a', 99, 0x00],
        expect: Expect::Reject("BadOffset"),
    });

    v.push(Vector {
        name: "header class wider than the length needs",
        note: "Declared 5 written in class 1. It decodes to the same value, and \
               is refused so that one length has exactly one encoding.",
        stream: {
            let mut s = vec![0x40, 0x05, 0x50];
            s.extend_from_slice(b"hello");
            s
        },
        expect: Expect::Reject("NonCanonicalHeader"),
    });

    v.push(Vector {
        name: "literal run reaching past the end of the stream",
        note: "The token claims eight literals and three bytes follow.",
        stream: vec![8, 0x80, b'a', b'b', b'c'],
        expect: Expect::Reject("Truncated"),
    });

    v.push(Vector {
        name: "body ends before the declared length",
        note: "Three bytes produced, thirty declared, nothing left to read.",
        stream: vec![30, 0x30, b'a', b'b', b'c'],
        expect: Expect::Reject("Truncated"),
    });

    // ---- Check every one of them, then write ----------------------------

    let mut out = String::new();
    out.push_str(HEADER);
    for vec in &v {
        check(vec);
        emit(&mut out, vec);
    }
    std::fs::write("docs/vectors.txt", &out).expect("write docs/vectors.txt");
    println!("{} vectors written to docs/vectors.txt", v.len());
}

/// The header, written the way the format does, so the vectors do not depend on
/// a hand-computed class.
fn put_header(declared: usize, hybrid: bool, out: &mut Vec<u8>) {
    let n = match declared {
        0..=63 => 1,
        64..=8191 => 2,
        8192..=2_097_151 => 3,
        _ => 4,
    };
    let class = (n - 1) as u8;
    let payload = declared as u32;
    let mut first = (class << 6) | (payload >> (8 * (n - 1))) as u8;
    if hybrid && n > 1 {
        first |= 0x20;
    }
    out.push(first);
    for i in (0..n - 1).rev() {
        out.push((payload >> (8 * i)) as u8);
    }
}

/// A vector that does not describe what the decoder does is not published.
fn check(v: &Vector) {
    let mut got = Vec::new();
    let result = lzvolt::decompress(&v.stream, &mut got);
    match &v.expect {
        Expect::Output(want) => {
            assert_eq!(result, Ok(()), "{}: refused", v.name);
            assert_eq!(&got, want, "{}", v.name);
        }
        Expect::Repeat(unit, count) => {
            assert_eq!(result, Ok(()), "{}: refused", v.name);
            let want: Vec<u8> = unit
                .iter()
                .cycle()
                .take(unit.len() * count)
                .copied()
                .collect();
            assert_eq!(got.len(), want.len(), "{}: length", v.name);
            assert_eq!(got, want, "{}", v.name);
        }
        Expect::Reject(name) => {
            let err = result.expect_err(&format!("{}: accepted", v.name));
            assert_eq!(format!("{err:?}"), *name, "{}", v.name);
        }
    }
}

fn hex(bytes: &[u8]) -> String {
    let mut s = String::new();
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 && i % 24 == 0 {
            s.push_str("\n  ");
        } else if i > 0 {
            s.push(' ');
        }
        let _ = write!(s, "{b:02x}");
    }
    s
}

fn emit(out: &mut String, v: &Vector) {
    for line in wrap(v.note, 74) {
        let _ = writeln!(out, "# {line}");
    }
    let _ = writeln!(out, "name: {}", v.name);
    let _ = writeln!(out, "stream: {}", hex(&v.stream));
    match &v.expect {
        Expect::Output(bytes) if bytes.is_empty() => out.push_str("output:\n"),
        Expect::Output(bytes) => {
            let _ = writeln!(out, "output: {}", hex(bytes));
        }
        Expect::Repeat(unit, count) => {
            let _ = writeln!(out, "output-repeat: {} * {count}", hex(unit));
        }
        Expect::Reject(name) => {
            let _ = writeln!(out, "reject: {name}");
        }
    }
    out.push('\n');
}

fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        if !line.is_empty() && line.len() + 1 + word.len() > width {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

const HEADER: &str = "\
# lzvolt decoder test vectors, version 1.
#
# Each record is a stream and what a conforming decoder must do with it. These
# are decoder vectors on purpose: what an encoder emits for a given input is
# its own choice, so pinning that would make a conforming implementation look
# wrong. See docs/FORMAT.md.
#
# Records are separated by blank lines. Lines beginning with '#' are comments.
# A field is `key: value`; a value continues on any following line that starts
# with whitespace. Bytes are lowercase hex, space separated.
#
#   name:            what the vector is for
#   stream:          the bytes handed to the decoder
#   output:          the bytes it must produce (may be empty)
#   output-repeat:   `<unit> * <count>`, the unit repeated, for large outputs
#   reject:          it must be refused, with this error
#
# Generated by `cargo run --example make_vectors`, and checked on every test
# run by `the_published_vectors_hold` in src/format.rs.

";
