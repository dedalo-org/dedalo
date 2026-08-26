//! `dedalo` — merge-to-earn funding for open source.
//!
//! The whole binary is [`dedalo::cli`]: keeping it in the library means the
//! command surface compiles under the same lints and is reachable from the
//! crate's own tests.

fn main() -> std::process::ExitCode {
    dedalo::cli::main()
}
