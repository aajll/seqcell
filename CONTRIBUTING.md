# Contributing to seqcell

`seqcell` is a sequence-lock cell for sharing a multi-word value between threads. A change must keep every shared payload access atomic and of a fixed width per byte, keep readers from ever writing to the cell, keep the payload bound at `NoUninit`, and stay `no_std` with `bytemuck` as its only runtime dependency. See [docs/design.md](docs/design.md) for the protocol and the rules the tests enforce.

## Getting started

These are the commands CI runs. Run them locally before opening a pull request.

```sh
# Format, lint and document with warnings denied
cargo fmt --check
cargo clippy --all-targets -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps

# Tests on stable and on the minimum supported Rust version
cargo test
cargo +1.85 test

# Every test under Miri, for 64-bit and 32-bit words and both byte orders
rustup +nightly component add miri rust-src
cargo +nightly miri test
cargo +nightly miri test --target i686-unknown-linux-gnu
cargo +nightly miri test --target s390x-unknown-linux-gnu

# Prove the library stays no_std
rustup target add thumbv7em-none-eabihf
cargo build --target thumbv7em-none-eabihf

# Coverage gate: 100% line and region coverage of src/
cargo install cargo-llvm-cov --locked
rustup component add llvm-tools-preview
cargo llvm-cov --all-targets --fail-under-lines 100 --fail-under-regions 100

# Dependency policy and advisories
cargo install cargo-deny cargo-audit --locked
cargo deny check
cargo audit
```

## Source style

- `rustfmt` with the default configuration; CI checks `cargo fmt --check`.
- Clippy `pedantic` is enabled through the `[lints]` table in `Cargo.toml` and CI denies warnings. An `#[allow]` carries a comment explaining why.
- Every source file starts with an SPDX licence tag.

## Language rules

- `#![no_std]` with no `alloc`. `bytemuck` is the only runtime dependency. A dev-dependency is acceptable when it earns its place in the tests.
- `unsafe` is confined to the payload copy and the `Sync` implementation. Every `unsafe` block and `unsafe fn` states its safety argument; `clippy::undocumented_unsafe_blocks` and `unsafe_op_in_unsafe_fn` are errors.
- Public items have rustdoc with an example where the behaviour is not obvious, and `missing_docs` is a warning that CI turns into an error.

## Tests and coverage

- `src/lib.rs` holds the implementation; unit tests belong there only for private parts, such as the chunk split and the counter.
- `tests/contract.rs` holds one test per rule in `docs/design.md`, named for the rule it falsifies. A behaviour change updates the design document and the test together.
- `tests/concurrency.rs` runs readers against busy writers and checks every accepted value against the seed it carries. A change to the protocol must keep these tests failing when the reader's counter comparison is removed.
- The README and the guides under `docs/` are compiled and run as doctests through `include_str!` in `src/lib.rs`, so an example that stops compiling fails `cargo test`. Mark a fenced block that is not runnable Rust with its real language (`toml`, `sh`, `text`).
- Coverage is gated at 100% line and region coverage of `src/`. A new branch comes with the test that reaches it.

## Documentation

| File | Purpose |
| --- | --- |
| `README.md` | Landing page: quick start, description, validated toolchains, and links. Keep it short. |
| `docs/integration.md` | Adding the dependency, payload types, sharing a cell, bounded reads, and cross builds. |
| rustdoc (`cargo doc`) | Reference for every public item. Changes with the interface. |
| `docs/design.md` | Protocol, memory ordering, payload copy, progress, decisions, cost and verification. |
| `CHANGELOG.md` | User-visible changes, under `[Unreleased]` until a release. |

Update rustdoc together with the interface. Update `docs/design.md` when behaviour changes, and record a design choice with its alternatives under its Decisions section. Write prose in British English, one line per paragraph, with no hard wrapping.

## Commits

Use Conventional Commits:

- `feat: ...` new feature
- `fix: ...` bug fix
- `docs: ...` documentation only
- `test: ...` test-only changes
- `chore: ...` build, CI, release work
- `refactor: ...` a change that neither fixes a bug nor adds a feature

Keep the subject under about 70 characters. Use the body to explain why the change is needed.

## Pull requests

- Open an issue first for non-trivial changes so the design can be agreed before implementation.
- Keep pull requests focused: one feature or one fix.
- All CI checks must pass.
