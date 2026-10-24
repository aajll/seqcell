# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.0] - 2026-10-24

The version follows the C `seqlock` 0.2.0, whose protocol this crate implements with its race-free payload copy only.

### Added

- `SeqCell`, a sequence-lock cell with `new`, `store`, `load` and `try_load`, usable as a `static`.
- A payload copy made entirely of atomic accesses, a word at a time, so an overlapping read is not a data race.
- Stores serialised by the sequence counter, so a store from any thread is safe and a single writer stays wait-free.
- A `bytemuck::NoUninit` payload bound, re-exported as `seqcell::NoUninit`, that rejects padding at compile time.
- Unit, contract and concurrency tests, with CI gates on formatting, clippy pedantic, rustdoc, native tests on x86-64 and AArch64, Miri on x86-64, i686 and s390x, a `thumbv7em-none-eabihf` build, `cargo deny`, `cargo audit` and 100% line and region coverage.
