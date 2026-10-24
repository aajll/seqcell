# AGENTS.md

---

## 1) Project-specific instructions

**Project:** `seqcell`
**Primary goal:** A sequence-lock cell in `no_std` Rust: a writer publishes a multi-word value and any number of readers copy it whole, with every payload access atomic so that there is no data race.

### 1.1 Essential commands

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps
cargo test
cargo +nightly miri test
cargo +nightly miri test --target i686-unknown-linux-gnu
cargo +nightly miri test --target s390x-unknown-linux-gnu
cargo build --target thumbv7em-none-eabihf
cargo llvm-cov --all-targets --fail-under-lines 100 --fail-under-regions 100
cargo deny check
cargo audit
```

### 1.2 Notes

- The README and `docs/*.md` are doctested through `include_str!` in `src/lib.rs`; a Rust fence there must compile and run.
- The crate is at `0.2.0`; a release bumps the version and the changelog together.

## 2) Non-negotiable design invariants

- Every access to the payload while the cell is shared is atomic, and each byte is always accessed at the same width and address. No `memcpy`, no volatile copy, no non-atomic read of shared payload bytes.
- A store claims the counter from even to odd with a compare-and-swap, issues a release fence, copies, and releases the counter at `s + 2`. A load attempt reads the counter with acquire ordering, rejects an odd value, copies, issues an acquire fence and accepts only an unchanged counter.
- A reader never writes the counter or the payload, so it never delays a writer.
- The payload bound stays `NoUninit`; an accepted copy is always the bytes of one complete store.
- Every `unsafe` block carries a `SAFETY:` comment; Miri passes on x86-64, i686 and s390x.
- `#![no_std]`, no `alloc`, one runtime dependency (`bytemuck`).

## 3) CI / source of truth

`.github/workflows/ci.yml` is the source of truth for the gates: format, clippy pedantic, rustdoc, tests on stable and the `rust-version` toolchain on x86-64 and AArch64, Miri on three targets, a `thumbv7em-none-eabihf` build, 100% line and region coverage, `cargo deny` and `cargo audit`.

## 4) Docs / commit conventions

### Documentation layout

- `README.md` is a short landing page. Detail goes in `docs/`.
- `docs/integration.md` covers adding the dependency, payload types, sharing a cell, MSRV and cross builds.
- Rustdoc is the API reference and changes with the interface.
- `docs/design.md` is the single design record; a behaviour change updates it and `tests/contract.rs` together, and a design choice goes under its Decisions section with the alternatives.
- Prose is British English, one line per paragraph, no hard wrapping, no em dashes joining clauses.

### Commits

Conventional Commits, subject under about 70 characters, body explains why. No tool attribution lines.

## 5) Rust style expectations

- Edition 2024, `rust-version = "1.85"`; do not use a feature newer than the MSRV.
- Clippy pedantic through `[lints]`; justify any `#[allow]` in a comment.
- Tests are named for the rule they falsify.
