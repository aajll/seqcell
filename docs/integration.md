# Integration guide

How to add `seqcell` to a Rust project. The [API reference](https://docs.rs/seqcell) describes every public item and [design.md](design.md) explains the behaviour.

## Requirements

- Rust 1.85 or later (edition 2024). The minimum is recorded as `rust-version` in `Cargo.toml` and checked in CI.
- One runtime dependency, `bytemuck`, for the payload bound. The crate enables its `min_const_generics` feature, so an array of any length is a payload.
- A target with compare-and-swap atomics of 8, 16 and 32 bits and of the pointer width. The crate refuses to build elsewhere.

## Git dependency

Until the crate is published, depend on a release tag:

```toml
[dependencies]
seqcell = { git = "https://github.com/aajll/seqcell", tag = "v0.2.0" }
```

Pinning a tag rather than a branch keeps builds reproducible and makes an upgrade a deliberate change.

## Published crate

Once published, use the version from crates.io:

```toml
[dependencies]
seqcell = "0.2"
```

## Payload types

A payload is any type that implements `NoUninit`: `Copy`, `'static`, and free of padding. The primitive integer and floating-point types, `bool`, `char` and arrays of them already implement it. For a structure, derive it with `bytemuck`'s `derive` feature, which fails to compile when the layout has padding:

```toml
[dependencies]
bytemuck = { version = "1.16", features = ["derive"] }
```

```rust
use bytemuck::NoUninit;
use seqcell::SeqCell;

#[derive(Clone, Copy, Debug, PartialEq, NoUninit)]
#[repr(C)]
struct Sample {
    timestamp_us: u64,
    value: f32,
    terminal: f32,
    valid: u8,
    // Explicit, so the structure has no padding.
    _reserved: [u8; 7],
}

let cache = SeqCell::new(Sample {
    timestamp_us: 0,
    value: 0.0,
    terminal: 0.0,
    valid: 0,
    _reserved: [0; 7],
});
let first = Sample { timestamp_us: 1_000, value: 2.5, terminal: 2.5, valid: 1, _reserved: [0; 7] };
cache.store(first);
assert_eq!(cache.load(), first);
```

A structure whose fields would leave padding can name it, as `_reserved` does above, or keep its public shape and convert to and from a padding-free form, such as an array of words, at the cell.

## Sharing a cell

A cell is shared by reference. Make it a `static`, place it in a structure behind an `Arc`, or borrow it into scoped threads:

```rust
use seqcell::SeqCell;
use std::sync::Arc;
use std::thread;

let cells: Arc<Vec<SeqCell<f64>>> = Arc::new((0..4).map(|_| SeqCell::new(0.0)).collect());

let worker = {
    let cells = Arc::clone(&cells);
    thread::spawn(move || {
        for (i, cell) in cells.iter().enumerate() {
            cell.store(i as f64 * 1.5);
        }
    })
};
worker.join().unwrap();
assert_eq!(cells[3].load(), 4.5);
```

Give each cell one writer where you can. Stores from several threads are correct but wait for each other. If the writers of neighbouring cells run on different cores, wrap each cell in a cache-line aligned type such as `crossbeam_utils::CachePadded`, so a store to one cell does not invalidate the line a reader of the next is using.

## Bounded reads

A caller that must not spin, such as a control loop with a deadline, reads with a bound and keeps its last good value:

```rust
use seqcell::SeqCell;

struct Reader {
    last_good: [u32; 4],
}

impl Reader {
    fn sample(&mut self, cell: &SeqCell<[u32; 4]>) -> [u32; 4] {
        if let Some(v) = cell.try_load(100) {
            self.last_good = v;
        }
        self.last_good
    }
}

let cell = SeqCell::new([1, 2, 3, 4]);
let mut reader = Reader { last_good: [0; 4] };
assert_eq!(reader.sample(&cell), [1, 2, 3, 4]);
```

On a single core where the reader is an interrupt handler and the writer runs at a lower priority, this is the only safe way to read: the handler can interrupt a store, and the store cannot finish until the handler returns, so `load` would never return.

## Build and test

```sh
cargo test
cargo clippy --all-targets -- -D warnings
cargo doc --no-deps
cargo +nightly miri test
```

See [CONTRIBUTING.md](../CONTRIBUTING.md) for the coverage, dependency and audit gates CI runs.

## Cross builds

CI builds the library for `thumbv7em-none-eabihf` to prove it stays `no_std`:

```sh
rustup target add thumbv7em-none-eabihf
cargo build --target thumbv7em-none-eabihf
```

Any target with the atomics listed under [requirements](#requirements) works. Tests run on the host, and under Miri for other word sizes and byte orders:

```sh
rustup +nightly component add miri rust-src
cargo +nightly miri test --target i686-unknown-linux-gnu
cargo +nightly miri test --target s390x-unknown-linux-gnu
```
