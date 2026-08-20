## What this changes

<!-- One paragraph. Link the issue if there is one. -->

## Why

<!-- What problem does it solve? -->

## Checklist

- [ ] `cargo test --workspace` passes
- [ ] `cargo clippy --all-targets --all-features -- -D warnings` is clean
- [ ] `cargo fmt --all` applied
- [ ] Public API changes are documented with rustdoc
- [ ] Anything touching money arithmetic, attribution or the fee split has a
      test proving the amounts still balance
