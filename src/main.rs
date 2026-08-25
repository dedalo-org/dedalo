//! The `dedalo` binary, for the placeholder release.
//!
//! `0.0.0` ships a binary at all so that the distribution path — the release
//! archives, `cargo install`, `cargo binstall` and `install.sh` — is exercised
//! by CI before there is anything worth distributing. It answers `--version`,
//! and otherwise says where the tool actually is.

fn main() {
    let version = env!("CARGO_PKG_VERSION");

    if std::env::args()
        .skip(1)
        .any(|arg| arg == "--version" || arg == "-V")
    {
        println!("dedalo {version}");
        return;
    }

    println!(
        "dedalo {version} is a placeholder. It reserves the name on crates.io \
         and does nothing else.\n\
         \n\
         The tool is being built on the v0.1 branch:\n\
         \n    \
         https://github.com/dedalo-org/dedalo/tree/v0.1\n\
         \n\
         The first release that carries code will be 0.0.1."
    );
}
