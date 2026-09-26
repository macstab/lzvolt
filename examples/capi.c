/*
 * Compiles and links against the built library, exactly as a user would.
 *
 * This is the only thing that proves include/lzvolt.h and src/ffi.rs agree:
 * a header is a claim about symbols, and the linker is what checks it. Run it
 * with `make c-example`.
 */

#include "lzvolt.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static int failures = 0;

static void check(int ok, const char *what) {
    printf("  %-46s %s\n", what, ok ? "ok" : "FAILED");
    if (!ok) {
        failures++;
    }
}

int main(void) {
    printf("lzvolt %s, decoding with %s\n\n", lzvolt_version(), lzvolt_backend());

    /* Something that compresses: a record shape repeated. */
    enum { N = 4096 };
    uint8_t *data = malloc(N);
    for (int i = 0; i < N; i++) {
        data[i] = (uint8_t) "{\"id\":42,\"role\":\"member\"} "[i % 26];
    }

    size_t cap = lzvolt_compress_bound(N);
    uint8_t *packed = malloc(cap);
    int64_t n = lzvolt_compress(data, N, packed, cap);
    check(n > 0, "compress returns a stream");
    check((size_t) n < N, "the stream is smaller than the input");
    printf("  %d bytes -> %lld (%.1f%%)\n", N, (long long) n, 100.0 * (double) n / N);

    int64_t declared = lzvolt_declared_size(packed, (size_t) n);
    check(declared == N, "the stream declares its own length");

    size_t out_cap = lzvolt_decompress_bound((size_t) declared);
    uint8_t *out = malloc(out_cap);
    int64_t got = lzvolt_decompress(packed, (size_t) n, out, out_cap);
    check(got == N, "decompress writes the declared length");
    check(memcmp(out, data, N) == 0, "the bytes come back identical");

    /* Something that does not compress. */
    uint64_t state = 0x9E3779B97F4A7C15u;
    uint8_t *noise = malloc(N);
    for (int i = 0; i < N; i++) {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        noise[i] = (uint8_t) state;
    }
    int64_t refused = lzvolt_compress(noise, N, packed, cap);
    check(refused == LZVOLT_NOT_COMPRESSIBLE, "incompressible input is declined");

    /* Bad arguments are codes, not crashes. */
    check(lzvolt_decompress(NULL, 8, out, out_cap) == LZVOLT_E_NULL,
          "a null source with a length is refused");
    check(lzvolt_decompress(packed, (size_t) n, out, 4) == LZVOLT_E_OUTPUT_TOO_SMALL,
          "a buffer that is too small is refused");
    check(lzvolt_declared_size(packed, 1) == LZVOLT_E_TRUNCATED,
          "a truncated stream is refused");

    /* Corrupt the body. What is guaranteed here is *not* that the corruption
     * is detected: the format carries no checksum, so flipping a byte can
     * produce a different stream that is entirely valid. What is guaranteed
     * is that nothing is read or written out of bounds, and that whatever
     * comes back is either an error or a length that fits the buffer. Add a
     * checksum above this layer if you need to know the bytes are yours. */
    uint8_t *broken = malloc((size_t) n);
    memcpy(broken, packed, (size_t) n);
    broken[(size_t) n / 2] ^= 0xFF;
    int64_t bad = lzvolt_decompress(broken, (size_t) n, out, out_cap);
    check(bad < 0 || (size_t) bad <= out_cap,
          "a corrupted stream stays inside the buffer");

    printf("\n  strerror(%d): %s\n", (int) LZVOLT_E_BAD_OFFSET,
           lzvolt_strerror(LZVOLT_E_BAD_OFFSET));

    free(data);
    free(packed);
    free(out);
    free(noise);
    free(broken);

    printf("\n%s\n", failures ? "FAILED" : "all checks passed");
    return failures ? 1 : 0;
}
