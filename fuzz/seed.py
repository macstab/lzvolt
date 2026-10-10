#!/usr/bin/env python3
"""Seed the fuzz corpora from the repository's own test vectors.

Random bytes almost always die at the header check, so an unseeded run spends
its budget proving the header reader works and never reaches the decode loop.
The published vectors in docs/vectors.txt are valid streams covering every
branch the format has, which is exactly the starting population the mutator
wants.

Run from the repository root:  python3 fuzz/seed.py
"""

import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
VECTORS = os.path.join(ROOT, "docs", "vectors.txt")
CORPUS = os.path.join(ROOT, "fuzz", "corpus")


def records(path):
    """Yield {key: value} dicts. Fields continue on indented lines."""
    rec, key = {}, None
    for raw in open(path, encoding="utf-8"):
        line = raw.rstrip("\n")
        if line.startswith("#"):
            continue
        if not line.strip():
            if rec:
                yield rec
            rec, key = {}, None
            continue
        if line[0].isspace() and key:
            rec[key] += " " + line.strip()
            continue
        if ":" in line:
            key, _, val = line.partition(":")
            key = key.strip()
            rec[key] = val.strip()
    if rec:
        yield rec


def unhex(s):
    return bytes(int(b, 16) for b in s.split()) if s.strip() else b""


def write(target, name, data):
    d = os.path.join(CORPUS, target)
    os.makedirs(d, exist_ok=True)
    slug = re.sub(r"[^a-z0-9]+", "-", name.lower()).strip("-")[:60] or "seed"
    with open(os.path.join(d, slug), "wb") as fh:
        fh.write(data)


def main():
    if not os.path.exists(VECTORS):
        sys.exit(f"no vectors at {VECTORS}")

    native = lz4 = rt = into = 0
    for rec in records(VECTORS):
        name = rec.get("name", "seed")
        stream = unhex(rec.get("stream", ""))
        if not stream:
            continue

        # decode_native takes the stream as-is.
        write("decode_native", name, stream)
        native += 1

        # decode_into prefixes one byte choosing how wrong the buffer is; seed
        # all four choices so each branch starts covered.
        for choice in range(4):
            write("decode_into", f"{name}-{choice}", bytes([choice]) + stream)
            into += 1

        # decode_lz4 prefixes a little-endian u16 that the target scales by 97.
        # Aim the declared length at the vector's real output size so the seed
        # is close to self-consistent rather than nonsense.
        out = unhex(rec.get("output", ""))
        want = max(1, round(len(out) / 97)) if out else 1
        write("decode_lz4", name, want.to_bytes(2, "little") + stream)
        lz4 += 1

        # roundtrip wants plaintext, not streams. The decoded side of each
        # vector is real data the packer will have opinions about.
        if out:
            write("roundtrip", name, out)
            rt += 1

    # A few shapes the vectors do not contain, because they are about volume
    # rather than branch coverage: the packer only engages above its probe
    # threshold, and only repetitive data survives worth_storing.
    extra = {
        "records-json": (b'{"id":%d,"name":"user","active":true,"tier":"gold"}' % 0) * 80,
        "runs": b"A" * 4096,
        "two-halves": (b"the quick brown fox " * 50) * 2,
        "ascending": bytes(range(256)) * 16,
        "sparse": (b"\x00" * 64 + b"\xff") * 64,
    }
    for name, data in extra.items():
        write("roundtrip", name, data)
        rt += 1

    print(f"decode_native {native}   decode_lz4 {lz4}   "
          f"decode_into {into}   roundtrip {rt}")


if __name__ == "__main__":
    main()
