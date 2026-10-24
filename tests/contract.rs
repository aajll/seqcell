// SPDX-License-Identifier: MIT
//! One test per rule in `docs/design.md`, named for the rule it falsifies.

use bytemuck::NoUninit;
use seqcell::SeqCell;

#[test]
fn a_new_cell_reads_its_initial_value() {
    let cell = SeqCell::new(41u32);
    assert_eq!(cell.load(), 41);
}

#[test]
fn a_default_cell_reads_the_default_value() {
    let cell = SeqCell::<[u16; 3]>::default();
    assert_eq!(cell.load(), [0; 3]);
}

#[test]
fn a_load_returns_the_latest_complete_store() {
    let cell = SeqCell::new(0u64);
    for v in [5, 6, u64::MAX] {
        cell.store(v);
        assert_eq!(cell.load(), v);
    }
}

#[test]
fn a_quiet_cell_loads_on_the_first_attempt() {
    let cell = SeqCell::new(3i8);
    cell.store(-3);
    assert_eq!(cell.try_load(1), Some(-3));
}

#[test]
fn zero_attempts_never_load() {
    let cell = SeqCell::new(3i8);
    assert_eq!(cell.try_load(0), None);
}

macro_rules! round_trip_sizes {
    ($($n:literal)*) => {$(
        let first: [u8; $n] = core::array::from_fn(|i| i.to_le_bytes()[0]);
        let second: [u8; $n] = core::array::from_fn(|i| !i.to_le_bytes()[0]);
        let cell = SeqCell::new(first);
        assert_eq!(cell.load(), first, "size {}", $n);
        cell.store(second);
        assert_eq!(cell.load(), second, "size {}", $n);
    )*};
}

#[test]
fn a_payload_of_every_size_round_trips_byte_for_byte() {
    round_trip_sizes!(0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 23 31 32 33 63 64 65 255);
}

#[derive(Clone, Copy, Debug, PartialEq, NoUninit)]
#[repr(C)]
struct Reading {
    timestamp_us: u64,
    value: f32,
    terminal: f32,
    valid: u8,
    quality: u8,
    channel: u16,
    _reserved: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, NoUninit)]
#[repr(u8)]
enum Mode {
    Off = 1,
    On = 200,
}

#[test]
fn a_payload_of_any_alignment_or_kind_round_trips() {
    let reading = Reading {
        timestamp_us: 1_234_567,
        value: 3.5,
        terminal: -0.25,
        valid: 1,
        quality: 2,
        channel: 9,
        _reserved: 0,
    };
    let cell = SeqCell::new(Reading {
        valid: 0,
        ..reading
    });
    cell.store(reading);
    assert_eq!(cell.load(), reading);

    let mode = SeqCell::new(Mode::Off);
    mode.store(Mode::On);
    assert_eq!(mode.load(), Mode::On);

    let wide = SeqCell::new(0u128);
    wide.store(u128::MAX - 1);
    assert_eq!(wide.load(), u128::MAX - 1);

    let flag = SeqCell::new(false);
    flag.store(true);
    assert!(flag.load());
}

static SHARED: SeqCell<[u32; 4]> = SeqCell::new([1, 2, 3, 4]);

#[test]
fn a_cell_can_be_a_static() {
    assert_eq!(SHARED.load(), [1, 2, 3, 4]);
}

#[test]
fn debug_shows_the_current_value() {
    let cell = SeqCell::new(12u8);
    assert_eq!(format!("{cell:?}"), "SeqCell(12)");
}

#[test]
fn a_cell_of_a_send_payload_is_send_and_sync() {
    fn shareable<T: Send + Sync>() {}
    shareable::<SeqCell<Reading>>();
}
