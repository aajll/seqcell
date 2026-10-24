# Design

`seqcell` is a sequence lock. A cell holds a sequence counter and a payload. A writer makes the counter odd, copies the new value in, and makes it even again; a reader copies the value out between two reads of the counter and keeps the copy only if the counter was even and did not change. Every payload access is atomic, so a read that overlaps a store is not a data race, and the copy it produced is discarded. The protocol is the one the C [`seqlock`](https://github.com/aajll/seqlock) library implements, restricted to its race-free payload copy.

## Goals and non-goals

Goals:

- Publish a value wider than one machine word to any number of readers, so that every reader gets the value of one complete store, never a mixture of two.
- Never make a writer wait for a reader, so a slow or stalled reader cannot delay the producer.
- No data race in the Rust memory model, so the crate is sound and Miri can check all of it.
- No allocation, no system calls, no operating-system locks and no global state; a cell can be a `static`.

Non-goals:

- A history or queue of values. A reader sees only the latest complete store, and a reader slower than the writer skips values.
- A wait-free reader, or fairness among readers. `try_load` is the bounded alternative to `load`.
- Payloads that need a deep copy or carry padding. The payload is copied byte for byte and must be free of uninitialised bytes.
- Cheap contended writes. Stores from several threads are correct but wait for each other; the design is for one writer per cell.

## Protocol

The counter is a `u32`. It starts at zero, is even while the payload is stable, and is odd while a store is in progress.

A store:

1. Claims the counter. It loads the counter and, if it is even at `s`, moves it to `s + 1` with a compare-and-swap. If the counter is odd, or the swap fails because another store claimed it first, it spins and tries again.
2. Issues a release fence (F1).
3. Copies the new value into the payload with relaxed atomic stores.
4. Stores `s + 2` with release ordering, marking the store complete.

A load attempt:

1. Loads the counter with acquire ordering, as `s1`. If `s1` is odd, a store is in progress and the attempt fails.
2. Copies the payload out with relaxed atomic loads.
3. Issues an acquire fence (F2).
4. Loads the counter again, as `s2`. If `s2` equals `s1`, no store began during the copy, and the attempt returns the copy. Otherwise it fails.

`load` repeats attempts until one succeeds. `try_load` makes at most the number of attempts it is given and returns `None` when they all fail, including when it is given none.

## Memory ordering

The acquire load of `s1` reads the value that step 4 of some store wrote. That release store synchronises with the acquire load, so every payload store of that store happens before the reader's copy, and the copy sees them.

The second check rejects a copy that read any part of a later store. Suppose the copy read a payload value written in step 3 of a later store. That value was written after F1 and read before F2, so F1 synchronises with F2. The later store's odd counter, written before F1, therefore happens before the reader's load of `s2`, which reads that value or a newer one, and so differs from `s1`. A copy that saw any part of a newer value always fails the check.

F1 must be a hardware fence on a weakly ordered CPU, not only a compiler barrier. Without it, the payload stores of step 3 could become visible to another core before the odd counter of step 1, and a reader could copy half a new value between two loads of the same even counter. F2 is the reader's mirror of it.

Two stores are ordered by the counter. A store's claim reads the even value that the previous store's step 4 released, and claims it with acquire ordering, so the previous store's payload writes happen before the next store's, and the payload ends with the bytes of the later one.

Because every payload access is atomic, this is the construction that Boehm proves correct in "Can Seqlocks Get Along With Programming Language Memory Models?" (MSPC 2012), with no access outside the memory model.

## Payload copy

The payload is copied in naturally aligned chunks: whole machine words, then at most one chunk each of 4, 2 and 1 bytes for the remainder. The cell aligns the payload for the widest atomic access, so every chunk is aligned, and a given payload type always splits the same way. The writer's copy-in and the reader's copy-out therefore access each byte with an atomic of the same width at the same address, and never mix access sizes on one location. The value being stored and the reader's local copy are ordinary memory, read and written without alignment requirements.

An atomic load must read initialised bytes, so the payload type may contain no padding. The bound is `bytemuck::NoUninit`, which is `Copy` and `'static` with no uninitialised bytes; its derive rejects a type with padding at compile time. The payload needs no other bound: a reader returns only a copy whose every byte came from one complete store, so the bytes always form a valid value, and `bool`, `char` and field-less enums are valid payloads.

## Progress

With one writer, a store is wait-free: its compare-and-swap succeeds at the first attempt because readers never write the counter, and the copy has a fixed length with no loop that depends on readers. With several writers, a store waits while another store is in progress, so writers block each other, but never on a reader.

A reader retries only when a store was in progress or began during its copy, so every retry corresponds to progress by a writer. A writer that stores continuously can make a reader retry many times. `core::hint::spin_loop` runs on every retry, which is `pause` on x86 and `yield` on AArch64.

A reader is not lock-free when a writer stops mid-store. If the writer's thread is suspended between steps 1 and 4, the counter stays odd and `load` spins until it resumes. A store contains no operation that can panic between its two counter stores, so a panicking writer cannot leave the counter odd. `try_load` bounds the work in every case.

## Counter wrap

The counter wraps through zero, and the parity and equality checks stay correct across the wrap. Each store adds 2, so a reader could accept a mixed copy only if exactly 2^31 stores completed during one payload copy and returned the counter to `s1`.

## Decisions

### Only the race-free copy

The C reference copies with `memcpy` by default and relies on the compiler keeping that copy between the fences; a copy that overlaps a store is then a data race, which C11 leaves undefined. Rust gives no such licence either, so the cell copies only with atomics. Copying a word at a time keeps the cost near `memcpy`: a 24-byte payload is three loads on a 64-bit target.

### Stores are serialised by the counter

The C reference makes concurrent stores undefined behaviour and leaves the single-writer rule to the caller. In Rust that would make `store` an `unsafe` function, or would let safe code tear a value into one that is not a valid `T`. Claiming the counter with a compare-and-swap instead of a plain store makes every store from safe code correct, and costs one uncontended compare-and-swap per store when there is one writer. Splitting the cell into a unique writer handle and shared readers would avoid even that, but it ties the writer's lifetime to the cell's and does not suit cells held in shared tables.

### The payload bound is `NoUninit`

The cell needs a payload with no uninitialised bytes, because it reads every byte atomically. `bytemuck::NoUninit` states exactly that, comes with a derive that checks it at compile time, and is already implemented for the primitive types and arrays. A crate-local `unsafe` trait would avoid the dependency but push the padding proof onto every caller. A `Pod` bound would be stricter than needed and would exclude `bool` and enums.

### No uniprocessor backend

The C reference falls back to a `volatile` counter without fences for compilers that lack atomics. Every Rust target the crate supports has atomics, and the fences cost little on a single core, so the crate has one implementation. It refuses to build for a target without compare-and-swap at 8, 16, 32 bits and pointer width.

### A fixed 32-bit counter

The C reference lets the counter's type be configured. A 32-bit counter is lock-free on every supported target, and its wrap interval of 2^31 stores within one read is beyond any realistic copy, so the type is fixed.

### No cache-line alignment option

The C reference can align each cell to a cache line. A caller who needs that wraps the cell in its own aligned type, such as `crossbeam_utils::CachePadded`, which keeps the cell as small as its payload.

## Portability

The crate is `#![no_std]`, uses only `core` and `bytemuck` without default features, and builds for any target with compare-and-swap atomics of 8, 16 and 32 bits and of the pointer width. CI builds it for `thumbv7em-none-eabihf`.

## Cost

A store is a counter load, a compare-and-swap, a fence, one atomic store per chunk of the payload, and a release store. A load attempt is two counter loads, a fence, and one atomic load per chunk. On x86-64 and AArch64, relaxed atomic loads and stores of a word compile to ordinary moves, so the copy costs what a word-by-word copy costs.

A cell is the payload plus a 4-byte counter, with the payload aligned to the pointer width. `SeqCell<[u64; 3]>` is 32 bytes on a 64-bit target.

## Verification

- Unit tests in `src/lib.rs` cover the private parts: the payload alignment, the chunk split for every length up to four words, a load that gives up or waits while the counter is held odd, a store that waits for a store in progress, and the counter's wrap through zero.
- `tests/contract.rs` holds one test per rule in this document, named for the rule it falsifies, including round trips of payloads of every size from 0 to 17 bytes and beyond, and of every alignment and kind.
- `tests/concurrency.rs` runs 3 readers against a writer of 2,000,000 stores of a 128-byte payload, a seed and 15 words derived from it, with unbounded and bounded reads, and 2 readers against 2 concurrent writers. Every accepted value is checked against its seed, so any mixed copy fails the test, and the cell must hold a last stored value afterwards. Removing the reader's comparison of `s1` and `s2` makes these tests fail with a mixed copy.
- CI runs the tests natively on x86-64 and on AArch64, whose weaker ordering is what the fences are for, and under Miri on x86-64, on `i686` for 32-bit words, and on `s390x` for a big-endian target. Miri checks every atomic access for data races and mixed sizes, and runs the concurrency tests with 200 stores.
- Doc examples in the README and the guides compile and run as doctests.
- CI gates on `cargo fmt`, clippy pedantic with warnings denied and every `unsafe` block justified, `cargo doc` with warnings denied, the stable and minimum toolchains, a `no_std` target build, `cargo deny`, `cargo audit` and 100% line and region coverage of `src/`.
