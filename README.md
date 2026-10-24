# seqcell

[![CI](https://github.com/aajll/seqcell/actions/workflows/ci.yml/badge.svg)](https://github.com/aajll/seqcell/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](./LICENSE)

**A sequence-lock cell for `no_std` Rust: a writer publishes a multi-word value and any number of readers copy it whole, without locks and without a data race.**

## Quick start

Add the crate as a git dependency; see the [integration guide](docs/integration.md) for the tag to pin and the other ways to depend on it.

```rust
use seqcell::SeqCell;
use std::thread;

// A position and velocity that must always be read as a pair.
let axis = SeqCell::new([0i32, 0]);

thread::scope(|s| {
    // The writer: a control loop publishing its latest state.
    s.spawn(|| {
        for step in 1..=1000 {
            axis.store([step, 2 * step]);
        }
    });
    // A reader: every copy is one complete store, never half of two.
    s.spawn(|| {
        let [position, velocity] = axis.load();
        assert_eq!(velocity, 2 * position);
    });
});
assert_eq!(axis.load(), [1000, 2000]);
```

## Description

`seqcell` shares a value wider than one machine word between threads when reads far outnumber writes and a reader must never hold up the writer. The writer marks a store in progress on a sequence counter, copies the value in, and marks it complete; a reader copies the value out and keeps the copy only if the counter shows no store overlapped it.

- `load` returns the value of one complete store, and `try_load` bounds the attempts for a caller that must not spin
- A reader never makes a writer wait, and with one writer a store finishes in a bounded number of steps
- Stores from several threads are serialised by the counter, so sharing the writer is safe, though not free
- The payload is copied with atomic accesses a word at a time, so an overlapping read is not a data race and Miri checks the whole crate
- Any `Copy` payload without padding, enforced at compile time through `bytemuck::NoUninit`
- `no_std`, no allocation, no locks and no system calls; the cell can be a `static`

The crate does not queue values or keep a history: a reader sees only the latest complete store and skips any it was too slow to see. It does not make a reader wait-free, because a reader retries while stores keep overlapping its copy.

## Validated toolchains

| Toolchain | Target | Validation |
| --- | --- | --- |
| Rust stable and 1.85 | `x86_64` and `aarch64` Linux | Unit, contract and concurrency tests, clippy pedantic, coverage gate |
| Rust nightly, Miri | `x86_64`, `i686` and `s390x` Linux | Every test under Miri, covering 64-bit and 32-bit words and both byte orders |
| Rust stable | `thumbv7em-none-eabihf` | `no_std` library build |

See [cross builds](docs/integration.md#cross-builds).

## Documentation

- [Integration guide](docs/integration.md) — adding the dependency, payload types, sharing a cell, and cross builds
- [API reference](https://docs.rs/seqcell) — rustdoc for every public item, or `cargo doc --open`
- [Design](docs/design.md) — protocol, memory ordering, progress, decisions, and verification
- [Changelog](CHANGELOG.md) — release history

## Contributing

Read [CONTRIBUTING.md](CONTRIBUTING.md) before you open a pull request. Report security issues as described in [SECURITY.md](.github/SECURITY.md).

## License

MIT. See [LICENSE](./LICENSE).
