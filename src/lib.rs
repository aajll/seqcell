// SPDX-License-Identifier: MIT
#![doc = include_str!("../README.md")]
#![no_std]

#[cfg(not(all(
    target_has_atomic = "8",
    target_has_atomic = "16",
    target_has_atomic = "32",
    target_has_atomic = "ptr"
)))]
compile_error!(
    "seqcell needs compare-and-swap atomics of 8, 16 and 32 bits and of the pointer width"
);

use core::cell::UnsafeCell;
use core::fmt;
use core::hint::spin_loop;
use core::mem::{MaybeUninit, size_of};
use core::sync::atomic::{AtomicU8, AtomicU16, AtomicU32, AtomicUsize, Ordering, fence};

/// The bound on a payload: `Copy`, `'static`, and free of padding and other
/// uninitialised bytes, re-exported from [`bytemuck`].
///
/// Derive it with `bytemuck`'s `derive` feature, which rejects a type with
/// padding at compile time.
pub use bytemuck::NoUninit;

/// The widest atomic access the payload copy uses.
const WORD: usize = size_of::<usize>();

/// A cell that publishes a value of `T` from a writer to any number of
/// readers.
///
/// [`store`](Self::store) replaces the value, and [`load`](Self::load)
/// copies out the value of one complete store, never a mixture of two. A
/// reader never makes a writer wait. Stores from several threads are
/// serialised by the cell, and with one writer a store finishes in a
/// bounded number of steps.
///
/// The payload is copied with atomic accesses, so a load that overlaps a
/// store is not a data race. A reader that sees the sequence counter move
/// during its copy discards the copy and tries again.
///
/// ```
/// use seqcell::SeqCell;
///
/// static POSITION: SeqCell<[i32; 2]> = SeqCell::new([0, 0]);
///
/// POSITION.store([120, -4]);
/// assert_eq!(POSITION.load(), [120, -4]);
/// ```
#[repr(C)]
pub struct SeqCell<T> {
    /// Even while the value is stable, odd while a store is in progress.
    seq: AtomicU32,
    /// Aligns `value` for the widest atomic access, so the copy can move
    /// whole words; it occupies no space.
    _align: [AtomicUsize; 0],
    value: UnsafeCell<T>,
}

// SAFETY: while the cell is shared, `value` is only read and written through
// `copy_in` and `copy_out`, whose accesses are all atomic and of the same
// width at each offset, so a load overlapping a store is not a data race.
// Stores are serialised by the counter, and a load returns only the bytes of
// one complete store. Sharing the cell therefore moves whole `T` values
// between threads, which `T: Send` permits.
unsafe impl<T: NoUninit + Send> Sync for SeqCell<T> {}

impl<T: NoUninit> SeqCell<T> {
    /// Creates a cell holding `value`.
    #[must_use]
    pub const fn new(value: T) -> Self {
        Self {
            seq: AtomicU32::new(0),
            _align: [],
            value: UnsafeCell::new(value),
        }
    }

    /// Replaces the value.
    ///
    /// A store waits only for a store already in progress on another
    /// thread, never for a reader. With one writer it finishes in a bounded
    /// number of steps.
    pub fn store(&self, value: T) {
        let s = self.claim();
        // Orders the odd counter before every payload store, so a reader
        // whose copy sees any byte of this value also sees the counter move.
        fence(Ordering::Release);
        // SAFETY: `value` points to a `T` in this cell, aligned for
        // `AtomicUsize` by `_align`, and the claimed counter excludes every
        // other store. `T: NoUninit`, so all `size_of::<T>()` bytes of the
        // argument are initialised.
        unsafe {
            copy_in(
                self.value.get().cast(),
                (&raw const value).cast(),
                size_of::<T>(),
            );
        }
        self.seq.store(s.wrapping_add(2), Ordering::Release);
    }

    /// Returns the value of the latest complete store, or the initial value
    /// before the first one.
    ///
    /// It retries while a store overlaps its copy, so it does not return
    /// while a writer is stopped part-way through a store. A caller that
    /// must not spin uses [`try_load`](Self::try_load).
    #[must_use]
    pub fn load(&self) -> T {
        loop {
            if let Some(value) = self.attempt() {
                return value;
            }
            spin_loop();
        }
    }

    /// Returns the value as [`load`](Self::load) does, making at most
    /// `attempts` attempts; `None` when every attempt overlapped a store,
    /// or when `attempts` is zero.
    #[must_use]
    pub fn try_load(&self, attempts: u32) -> Option<T> {
        for _ in 0..attempts {
            if let Some(value) = self.attempt() {
                return Some(value);
            }
            spin_loop();
        }
        None
    }

    /// Moves the counter from even to odd for this store and returns the
    /// even value it held.
    fn claim(&self) -> u32 {
        loop {
            let s = self.seq.load(Ordering::Relaxed);
            if s & 1 == 0
                && self
                    .seq
                    .compare_exchange(s, s.wrapping_add(1), Ordering::Acquire, Ordering::Relaxed)
                    .is_ok()
            {
                return s;
            }
            spin_loop();
        }
    }

    /// One read: the copy, if no store was in progress or began during it.
    fn attempt(&self) -> Option<T> {
        let s1 = self.seq.load(Ordering::Acquire);
        if s1 & 1 != 0 {
            return None;
        }
        let mut out = MaybeUninit::<T>::uninit();
        // SAFETY: `value` points to a `T` in this cell, aligned for
        // `AtomicUsize` by `_align`, and `out` has room for one `T`.
        unsafe {
            copy_out(
                out.as_mut_ptr().cast(),
                self.value.get().cast(),
                size_of::<T>(),
            );
        }
        // Orders every payload load before the second counter load, so a
        // copy that saw any byte of a later store sees the counter move.
        fence(Ordering::Acquire);
        let s2 = self.seq.load(Ordering::Relaxed);
        // SAFETY: the counter was even and unchanged across the copy, so no
        // store overlapped it and `out` holds every byte of the `T` of one
        // complete store.
        (s1 == s2).then(|| unsafe { out.assume_init() })
    }
}

impl<T: NoUninit + Default> Default for SeqCell<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}

impl<T: NoUninit + fmt::Debug> fmt::Debug for SeqCell<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("SeqCell").field(&self.load()).finish()
    }
}

/// Calls `f` with the offset and width of each chunk of a `len`-byte copy:
/// whole words, then at most one chunk each of 4, 2 and 1 bytes. From a
/// word-aligned start every chunk is naturally aligned, and a given `len`
/// always splits the same way.
fn for_each_chunk(len: usize, mut f: impl FnMut(usize, usize)) {
    let mut at = 0;
    for width in [WORD, 4, 2, 1] {
        while len - at >= width {
            f(at, width);
            at += width;
        }
    }
}

/// Copies `len` bytes from `src` into the cell's storage at `dst` with
/// relaxed atomic stores.
///
/// # Safety
///
/// `dst` is aligned for `AtomicUsize` and valid for `len` bytes that, while
/// shared, are accessed only by `copy_in` and `copy_out` with the same `len`.
/// `src` is valid for reads of `len` initialised bytes.
unsafe fn copy_in(dst: *mut u8, src: *const u8, len: usize) {
    for_each_chunk(len, |at, width| {
        // SAFETY: `at + width <= len`, and `dst + at` is aligned to `width`
        // because `dst` is word aligned and every chunk before it is at
        // least `width` long. The source needs no alignment.
        unsafe {
            let (d, s) = (dst.add(at), src.add(at));
            match width {
                1 => AtomicU8::from_ptr(d).store(s.read(), Ordering::Relaxed),
                2 => AtomicU16::from_ptr(d.cast())
                    .store(s.cast::<u16>().read_unaligned(), Ordering::Relaxed),
                4 => AtomicU32::from_ptr(d.cast())
                    .store(s.cast::<u32>().read_unaligned(), Ordering::Relaxed),
                _ => AtomicUsize::from_ptr(d.cast())
                    .store(s.cast::<usize>().read_unaligned(), Ordering::Relaxed),
            }
        }
    });
}

/// Copies `len` bytes from the cell's storage at `src` into `dst` with
/// relaxed atomic loads.
///
/// # Safety
///
/// `src` is aligned for `AtomicUsize` and valid for `len` bytes that, while
/// shared, are accessed only by `copy_in` and `copy_out` with the same `len`.
/// `dst` is valid for writes of `len` bytes.
unsafe fn copy_out(dst: *mut u8, src: *mut u8, len: usize) {
    for_each_chunk(len, |at, width| {
        // SAFETY: as for `copy_in`, with the roles of the pointers swapped.
        unsafe {
            let (d, s) = (dst.add(at), src.add(at));
            match width {
                1 => d.write(AtomicU8::from_ptr(s).load(Ordering::Relaxed)),
                2 => d
                    .cast::<u16>()
                    .write_unaligned(AtomicU16::from_ptr(s.cast()).load(Ordering::Relaxed)),
                4 => d
                    .cast::<u32>()
                    .write_unaligned(AtomicU32::from_ptr(s.cast()).load(Ordering::Relaxed)),
                _ => d
                    .cast::<usize>()
                    .write_unaligned(AtomicUsize::from_ptr(s.cast()).load(Ordering::Relaxed)),
            }
        }
    });
}

#[cfg(doctest)]
#[doc = include_str!("../docs/integration.md")]
struct IntegrationGuide;

#[cfg(doctest)]
#[doc = include_str!("../docs/design.md")]
struct Design;

#[cfg(test)]
mod tests {
    extern crate std;

    use super::{SeqCell, WORD, for_each_chunk};
    use core::sync::atomic::{AtomicUsize, Ordering};
    use core::time::Duration;
    use std::thread;
    use std::vec::Vec;

    const HOLD: Duration = Duration::from_millis(20);

    #[test]
    fn the_payload_is_aligned_for_the_widest_access() {
        let cell = SeqCell::new(0u8);
        assert_eq!(cell.value.get().addr() % align_of::<AtomicUsize>(), 0);
    }

    #[test]
    fn every_chunk_is_naturally_aligned_and_the_chunks_tile_the_payload() {
        for len in 0..=4 * WORD {
            let mut next = 0;
            for_each_chunk(len, |at, width| {
                assert_eq!(at, next);
                assert_eq!(at % width, 0);
                next = at + width;
            });
            assert_eq!(next, len);
        }
    }

    #[test]
    fn a_tail_takes_at_most_one_chunk_of_each_narrower_width() {
        let mut widths = Vec::new();
        for_each_chunk(2 * WORD + 7, |_, width| widths.push(width));
        let narrow: Vec<_> = widths.iter().copied().filter(|&w| w < WORD).collect();
        assert_eq!(narrow, [4, 2, 1][usize::from(WORD == 4)..]);
    }

    #[test]
    fn a_load_while_a_store_is_in_progress_gives_up_after_its_attempts() {
        let cell = SeqCell::new(7u32);
        cell.seq.store(1, Ordering::Relaxed);
        assert_eq!(cell.try_load(3), None);
        cell.seq.store(2, Ordering::Relaxed);
        assert_eq!(cell.try_load(1), Some(7));
    }

    #[test]
    fn a_load_waits_for_a_store_in_progress_to_finish() {
        let cell = SeqCell::new(7u32);
        cell.seq.store(1, Ordering::Relaxed);
        thread::scope(|s| {
            let reader = s.spawn(|| cell.load());
            thread::sleep(HOLD);
            cell.seq.store(2, Ordering::Release);
            assert_eq!(reader.join().unwrap(), 7);
        });
    }

    #[test]
    fn a_store_waits_for_a_store_in_progress_on_another_thread() {
        let cell = SeqCell::new(7u32);
        cell.seq.store(1, Ordering::Relaxed);
        thread::scope(|s| {
            let writer = s.spawn(|| cell.store(9));
            thread::sleep(HOLD);
            assert_eq!(cell.seq.load(Ordering::Relaxed), 1);
            cell.seq.store(2, Ordering::Release);
            writer.join().unwrap();
        });
        assert_eq!(cell.seq.load(Ordering::Relaxed), 4);
        assert_eq!(cell.load(), 9);
    }

    #[test]
    fn the_counter_wraps_through_zero() {
        let cell = SeqCell::new([0u64; 3]);
        cell.seq.store(u32::MAX - 3, Ordering::Relaxed);
        for i in 0..4 {
            cell.store([i, i + 1, i + 2]);
            assert_eq!(cell.try_load(1), Some([i, i + 1, i + 2]));
            assert_eq!(cell.seq.load(Ordering::Relaxed) & 1, 0);
        }
        assert_eq!(cell.seq.load(Ordering::Relaxed), 4);
    }
}
