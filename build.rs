use std::env;
use std::path::Path;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=asm");
    println!("cargo:rustc-check-cfg=cfg(lzvolt_asm)");

    // Before the `asm` early return: the two features are independent.
    if env::var_os("CARGO_FEATURE_LIBLZ4").is_some() {
        find_liblz4();
    }

    if env::var_os("CARGO_FEATURE_ASM").is_none() {
        return;
    }

    let arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap();
    let sources: &[&str] = match arch.as_str() {
        "aarch64" => &[
            "asm/aarch64/pack_find.S",
            "asm/aarch64/pack.S",
            "asm/aarch64/unpack.S",
            "asm/aarch64/unpack_wide.S",
            "asm/aarch64/unpack_lz4.S",
            "asm/aarch64/unpack_lz4_neoverse.S",
        ],
        "x86_64" => &[
            "asm/x86_64/pack.S",
            "asm/x86_64/pack_xeon.S",
            "asm/x86_64/pack_amd.S",
            "asm/x86_64/unpack.S",
            "asm/x86_64/unpack_wide.S",
            "asm/x86_64/unpack_ssse3.S",
            "asm/x86_64/unpack_wide_ssse3.S",
            "asm/x86_64/unpack_xeon.S",
            "asm/x86_64/unpack_wide_xeon.S",
            "asm/x86_64/unpack_epyc.S",
            "asm/x86_64/unpack_wide_epyc.S",
            "asm/x86_64/unpack_lz4.S",
            "asm/x86_64/unpack_lz4_ssse3.S",
            "asm/x86_64/unpack_lz4_xeon.S",
            "asm/x86_64/unpack_lz4_epyc.S",
        ],
        // Every other target runs the scalar reference. That is a supported
        // configuration, not a degraded one -- correctness does not depend on
        // the kernels existing.
        _ => return,
    };

    let mut build = cc::Build::new();
    build.files(sources);

    // The `.S` extension means the assembler runs the C preprocessor first,
    // which is how the files pick the right symbol decoration for Mach-O
    // versus ELF.
    build.compile("lzvolt_asm_kernels");

    println!("cargo:rustc-cfg=lzvolt_asm");
}

/// Tell the linker where the platform keeps liblz4.
///
/// Only the comparison tests link it, and they already ask for it themselves
/// with `#[link(name = "lz4")]`. What was missing is the search path, and on a
/// Mac with Homebrew it is missing by default: `cargo test --features liblz4`
/// failed with `ld: library 'lz4' not found` while the library sat in
/// `/opt/homebrew/lib`, and nothing in the error said so. A test stayed red for
/// a day behind that.
///
/// Three sources, in order of how much they know. `pkg-config` is asked first
/// because it is the one that actually knows, but it is not enough on its own:
/// Homebrew's lz4 is keg-only, so `pkg-config --libs lz4` fails on exactly the
/// machine that has the library. Hence the prefix scan.
///
/// Twenty lines rather than a build dependency, which is the trade the rest of
/// this crate makes.
fn find_liblz4() {
    println!("cargo:rerun-if-env-changed=LZ4_LIB_DIR");
    println!("cargo:rerun-if-env-changed=PKG_CONFIG_PATH");

    // An explicit override wins, for building against an liblz4 that is not
    // the system's -- a different version, or a cross-compiled sysroot.
    if let Some(dir) = env::var_os("LZ4_LIB_DIR") {
        println!("cargo:rustc-link-search=native={}", dir.to_string_lossy());
        return;
    }

    if let Ok(out) = Command::new("pkg-config")
        .args(["--libs-only-L", "lz4"])
        .output()
    {
        if out.status.success() {
            // Success with no `-L` at all means the library is on the default
            // path, which needs nothing from us either way.
            for dir in String::from_utf8_lossy(&out.stdout)
                .split_whitespace()
                .filter_map(|token| token.strip_prefix("-L").map(str::to_owned))
            {
                println!("cargo:rustc-link-search=native={dir}");
            }
            return;
        }
    }

    // Where the package managers that matter put it. Checked for an actual
    // file rather than just the directory, so a bare `/usr/local/lib` on a
    // machine without lz4 does not silently become the answer.
    for dir in ["/opt/homebrew/lib", "/usr/local/lib", "/opt/local/lib"] {
        let base = Path::new(dir);
        if ["liblz4.dylib", "liblz4.so", "liblz4.a"]
            .iter()
            .any(|f| base.join(f).exists())
        {
            println!("cargo:rustc-link-search=native={dir}");
            return;
        }
    }

    println!(
        "cargo:warning=feature `liblz4` is on but liblz4 was not found. \
         Install it (brew install lz4, apt install liblz4-dev) or point \
         LZ4_LIB_DIR at the directory holding it."
    );
}
