---
description: Run every check CI runs, locally, and report what fails
allowed-tools: Bash(cargo fmt:*), Bash(cargo clippy:*), Bash(cargo test:*), Bash(cargo doc:*), Bash(nix flake check:*)
---

Run the same gates CI runs, in this order, and stop reporting success early if
any of them fails:

1. `cargo fmt --all --check`
2. `cargo clippy --workspace --all-targets --all-features -- -D warnings`
3. `cargo test --workspace --all-features`
4. `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --all-features`

Then summarise: for each gate, pass or fail, and for failures the exact error
with its file and line. Do not fix anything unless asked — report first.

If the working tree also changed `flake.nix`, `rust-toolchain.toml` or any
`Cargo.toml`, additionally run `nix flake check --print-build-logs`.
