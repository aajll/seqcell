// SPDX-License-Identifier: MIT
//! Readers against busy writers. Every value a reader accepts is checked
//! against the seed it carries, so a copy that mixed two stores fails.

use seqcell::SeqCell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

/// Stores per test; Miri interprets every access, so it runs fewer.
const STORES: u64 = if cfg!(miri) { 200 } else { 2_000_000 };

/// A 128-byte payload: a seed and 15 words derived from it.
type Payload = [u64; 16];

fn payload(seed: u64) -> Payload {
    core::array::from_fn(|i| {
        let k = u32::try_from(i).unwrap();
        if k == 0 {
            seed
        } else {
            seed.wrapping_mul(0x9E37_79B9_7F4A_7C15).rotate_left(k) ^ u64::from(k)
        }
    })
}

fn whole(v: &Payload) -> bool {
    *v == payload(v[0])
}

/// Runs `writers` writers, each storing `STORES / writers` values from its
/// own range of seeds, against `readers` readers, and returns the final
/// value.
fn run(writers: u64, readers: usize, bounded: bool) -> Payload {
    let cell = SeqCell::new(payload(0));
    let done = AtomicBool::new(false);
    thread::scope(|s| {
        for _ in 0..readers {
            s.spawn(|| {
                while !done.load(Ordering::Relaxed) {
                    let v = if bounded {
                        cell.try_load(1)
                    } else {
                        Some(cell.load())
                    };
                    if let Some(v) = v {
                        assert!(whole(&v), "mixed copy: {v:?}");
                    }
                }
            });
        }
        let handles: Vec<_> = (0..writers)
            .map(|w| {
                let cell = &cell;
                s.spawn(move || {
                    for i in 1..=STORES / writers {
                        cell.store(payload((w << 48) | i));
                    }
                })
            })
            .collect();
        for h in handles {
            h.join().unwrap();
        }
        done.store(true, Ordering::Relaxed);
    });
    cell.try_load(1)
        .expect("the counter is even once the writers finish")
}

#[test]
fn a_reader_never_accepts_a_mixed_value() {
    assert_eq!(run(1, 3, false), payload(STORES));
}

#[test]
fn a_bounded_reader_never_accepts_a_mixed_value() {
    assert_eq!(run(1, 3, true), payload(STORES));
}

#[test]
fn concurrent_writers_never_mix_their_values() {
    let last = run(2, 2, false);
    assert!([payload(STORES / 2), payload((1 << 48) | (STORES / 2))].contains(&last));
}
