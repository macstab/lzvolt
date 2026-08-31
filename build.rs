use std::env;

fn main() {
    println!("cargo:rerun-if-changed=asm");
    println!("cargo:rustc-check-cfg=cfg(keva_asm)");

    if env::var_os("CARGO_FEATURE_ASM").is_none() {
        return;
    }

    let arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap();
    let sources: &[&str] = match arch.as_str() {
        "aarch64" => &["asm/aarch64/ctrl_match.S"],
        "x86_64" => &["asm/x86_64/ctrl_match.S"],
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
    build.compile("keva_asm_kernels");

    println!("cargo:rustc-cfg=keva_asm");
}
