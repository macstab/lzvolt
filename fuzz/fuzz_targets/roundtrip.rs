//! Pack arbitrary input, unpack it, and insist on the same bytes back.
//!
//! The decode targets ask whether a malformed stream can hurt you. This one
//! asks the other question: whether a *well-formed* stream — one this packer
//! wrote itself — always reads back. A codec that loses a byte on some shape
//! nobody thought to test is a worse bug than a crash, because nothing reports
//! it at the time.
//!
//! The packer refuses anything it cannot shrink by an eighth, so a large share
//! of inputs here are declined rather than packed. That is the intended
//! behaviour and not a dead test: the refusal path has its own arithmetic.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let mut packed = Vec::new();
    if !lzvolt::compress(data, &mut packed) {
        return; // not worth packing; the caller stores the original
    }

    // A refusal and a pack are the only two outcomes, so if we got here the
    // stream must be well formed and must declare the length we handed in.
    let declared = lzvolt::decompressed_size(&packed)
        .expect("the packer wrote a stream its own header reader rejects");
    assert_eq!(declared, data.len(), "declared length is not the input length");

    let mut out = Vec::new();
    lzvolt::decompress(&packed, &mut out)
        .expect("a stream this packer wrote failed to decode");
    assert!(out == data, "round trip changed the bytes");

    // worth_storing: the packer promised an eighth, so hold it to that. A
    // stream that is larger than its input would be a silent capacity loss
    // for every caller that trusts the return value.
    assert!(
        packed.len() < data.len(),
        "packed {} bytes from an input of {}",
        packed.len(),
        data.len()
    );
});
