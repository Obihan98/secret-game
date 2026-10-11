//! Build-order stage 3 gate: input log, snapshots and replay.

// `allow-unwrap-in-tests` covers `#[test]` functions only, not this file's fixture helpers.
#![allow(clippy::unwrap_used)]

use std::fmt::Write;

use proptest::prelude::*;
use sim::codec::DecodeError;
use sim::fixed::Fixed;
use sim::id::{NationId, ProvinceId};
use sim::log::{AdminAction, Entry, FORMAT_VERSION, Record, Recorder, ReplayError};
use sim::order::Order;
use sim::rng::{Rng, StreamId};
use sim::time::{GameTime, SECONDS_PER_DAY};
use sim::world::Province;
use sim::world::SimError;

const DAY: i64 = SECONDS_PER_DAY;
const YEAR: i64 = 365 * DAY;

fn provinces() -> [Province; 3] {
    [
        Province {
            owner: NationId(0),
            base_income: Fixed::from_int(10),
        },
        Province {
            owner: NationId(0),
            base_income: Fixed::from_ratio(5, 2).unwrap(),
        },
        Province {
            owner: NationId(1),
            base_income: Fixed::from_int(7),
        },
    ]
}

fn create(seed: u64) -> Recorder {
    Recorder::create(seed, 2, &provinces()).unwrap()
}

#[derive(Clone, Copy, Debug)]
enum Op {
    Order(Order),
    Admin(AdminAction),
    Snapshot,
    Checkpoint,
}

/// A scripted live session: random ops at random times in `0..=end`, sorted by time. The
/// script's own randomness comes from a stream no sim system uses.
fn script(seed: u64, end: i64, counts: [usize; 4]) -> Vec<(i64, Op)> {
    let mut rng = Rng::new(seed, StreamId(1_000));
    let mut pick = |n: u64| rng.next_u64() % n;
    let mut ops = Vec::new();
    let span = u64::try_from(end).unwrap() + 1;
    for (kind, &count) in counts.iter().enumerate() {
        for _ in 0..count {
            let t = i64::try_from(pick(span)).unwrap();
            let op = match kind {
                0 => Op::Order(Order::TransferProvince {
                    province: ProvinceId(u32::try_from(pick(3)).unwrap()),
                    to: NationId(u32::try_from(pick(2)).unwrap()),
                }),
                1 => Op::Admin(match pick(3) {
                    0 => AdminAction::Pause,
                    1 => AdminAction::Resume,
                    _ => AdminAction::SetRatio(u32::try_from(pick(48)).unwrap() + 1),
                }),
                2 => Op::Snapshot,
                _ => Op::Checkpoint,
            };
            ops.push((t, op));
        }
    }
    ops.sort_by_key(|&(t, _)| t);
    ops
}

struct Checkpoint {
    at: i64,
    log_len: usize,
    /// The live snapshot at this moment: the whole state, compared byte for byte.
    state: Vec<u8>,
    /// Index into the snapshot list of the latest snapshot taken before this checkpoint.
    latest_snapshot: usize,
}

struct Session {
    live: Recorder,
    /// `snapshots[0]` is taken at creation.
    snapshots: Vec<Vec<u8>>,
    checkpoints: Vec<Checkpoint>,
}

fn run(seed: u64, end: i64, counts: [usize; 4]) -> Session {
    let mut live = create(seed);
    let mut snapshots = vec![live.snapshot()];
    let mut checkpoints = Vec::new();
    for (t, op) in script(seed, end, counts) {
        live.advance_to(GameTime(t)).unwrap();
        match op {
            // A random transfer may hand a province to its owner; that is rejected, not logged.
            Op::Order(order) => {
                let result = live.submit(order);
                assert!(
                    matches!(result, Ok(()) | Err(SimError::NoChange)),
                    "{result:?}"
                );
            }
            Op::Admin(action) => live.admin(action).unwrap(),
            Op::Snapshot => snapshots.push(live.snapshot()),
            Op::Checkpoint => checkpoints.push(Checkpoint {
                at: t,
                log_len: live.log().len(),
                state: live.snapshot(),
                latest_snapshot: snapshots.len() - 1,
            }),
        }
    }
    live.advance_to(GameTime(end)).unwrap();
    Session {
        live,
        snapshots,
        checkpoints,
    }
}

fn replay_to(snapshot: &[u8], log: &[Record], at: i64) -> Vec<u8> {
    let mut rec = Recorder::replay(snapshot, log).unwrap();
    rec.advance_to(GameTime(at)).unwrap();
    rec.snapshot()
}

#[test]
fn replay_equals_live_state_at_1000_random_times() {
    let s = run(42, YEAR, [300, 30, 12, 1_000]);
    let log = s.live.log();
    assert_eq!(s.checkpoints.len(), 1_000);
    for c in &s.checkpoints {
        let prefix = &log[..c.log_len];
        assert_eq!(
            replay_to(&s.snapshots[0], prefix, c.at),
            c.state,
            "t={}",
            c.at
        );
        let latest = &s.snapshots[c.latest_snapshot];
        assert_eq!(replay_to(latest, prefix, c.at), c.state, "t={}", c.at);
    }
    // The session did exercise every record kind.
    let kinds = |f: fn(&Entry) -> bool| log.iter().filter(|r| f(&r.entry)).count();
    assert!(kinds(|e| matches!(e, Entry::Order(_))) > 100);
    assert_eq!(kinds(|e| matches!(e, Entry::Admin(_))), 30);
    assert_eq!(kinds(|e| matches!(e, Entry::DayHash { .. })), 365);
}

#[test]
fn every_flipped_bit_is_caught_by_the_next_daily_hash() {
    let s = run(7, 10 * DAY, [40, 6, 0, 0]);
    let log = s.live.log();
    let snapshot = &s.snapshots[0];
    assert!(matches!(log.last().unwrap().entry, Entry::DayHash { .. }));
    // The creation snapshot already covers the ruleset and seed records.
    let covered = 2;

    let mut flips = 0;
    for i in covered..log.len() {
        let next_hash = (i..log.len())
            .find(|&j| matches!(log[j].entry, Entry::DayHash { .. }))
            .unwrap();
        let bytes = log[i].encode();
        for bit in 0..bytes.len() * 8 {
            let mut flipped = bytes.clone();
            flipped[bit / 8] ^= 1 << (bit % 8);
            flips += 1;
            let Ok(record) = Record::decode(&flipped) else {
                continue; // caught on read
            };
            assert_ne!(record, log[i]);
            let mut corrupt = log.to_vec();
            corrupt[i] = record;
            let err = Recorder::replay(snapshot, &corrupt).unwrap_err();
            let at = err.record_index().unwrap();
            assert!(
                (i..=next_hash).contains(&at),
                "record {i} bit {bit}: {err} at {at}, next hash {next_hash}"
            );
        }
    }
    assert!(flips > 5_000);
}

#[test]
fn transfer_settles_both_nations_at_that_second() {
    let mut rec = create(42);
    let t = DAY / 2;
    rec.advance_to(GameTime(t)).unwrap();
    let before: Vec<_> = rec.world().nations().to_vec();
    let order = Order::TransferProvince {
        province: ProvinceId(2),
        to: NationId(0),
    };
    rec.submit(order).unwrap();

    let w = rec.world();
    assert_eq!(w.provinces()[2].owner, NationId(0));
    assert_eq!(
        w.nations()[0].income_per_day,
        Fixed::from_ratio(39, 2).unwrap()
    );
    assert_eq!(w.nations()[1].income_per_day, Fixed::ZERO);
    for (n, old) in w.nations().iter().zip(&before) {
        let elapsed = t - old.settled_at.seconds();
        let earned = old.income_per_day.mul_ratio(elapsed, DAY).unwrap();
        assert_eq!(n.settled_at, GameTime(t));
        assert_eq!(n.treasury, old.treasury.checked_add(earned).unwrap());
    }
    assert_eq!(
        rec.log().last(),
        Some(&Record {
            at: GameTime(t),
            entry: Entry::Order(order)
        })
    );
}

#[test]
fn invalid_orders_change_nothing_and_are_not_logged() {
    let mut rec = create(1);
    let (hash, len) = (rec.world().state_hash(), rec.log().len());
    let bad = [
        (
            Order::TransferProvince {
                province: ProvinceId(3),
                to: NationId(0),
            },
            SimError::UnknownProvince(ProvinceId(3)),
        ),
        (
            Order::TransferProvince {
                province: ProvinceId(0),
                to: NationId(2),
            },
            SimError::UnknownNation(NationId(2)),
        ),
        (
            Order::TransferProvince {
                province: ProvinceId(0),
                to: NationId(0),
            },
            SimError::NoChange,
        ),
    ];
    for (order, err) in bad {
        assert_eq!(rec.submit(order), Err(err));
    }
    assert_eq!(rec.world().state_hash(), hash);
    assert_eq!(rec.log().len(), len);
}

#[test]
fn a_logged_order_that_no_longer_applies_stops_replay() {
    let s = run(3, 2 * DAY, [5, 0, 0, 0]);
    let mut log = s.live.log().to_vec();
    // Hand province 0 to its current owner at the very start: valid bytes, invalid order.
    log.insert(
        2,
        Record {
            at: GameTime(0),
            entry: Entry::Order(Order::TransferProvince {
                province: ProvinceId(0),
                to: NationId(0),
            }),
        },
    );
    assert_eq!(
        Recorder::replay(&s.snapshots[0], &log).unwrap_err(),
        ReplayError::Sim(2, SimError::NoChange)
    );
}

#[test]
fn replay_rejects_other_ruleset_versions() {
    let rec = create(1);
    let mut snapshot = rec.snapshot();
    snapshot[2..6].copy_from_slice(&2u32.to_le_bytes());
    assert_eq!(
        Recorder::replay(&snapshot, rec.log()).unwrap_err(),
        ReplayError::Ruleset {
            index: None,
            found: 2
        }
    );

    let mut log = rec.log().to_vec();
    log.push(Record {
        at: GameTime(5),
        entry: Entry::Ruleset(2),
    });
    assert_eq!(
        Recorder::replay(&rec.snapshot(), &log).unwrap_err(),
        ReplayError::Ruleset {
            index: Some(2),
            found: 2
        }
    );
}

#[test]
fn replay_rejects_short_logs_and_bad_snapshots() {
    let mut rec = create(1);
    rec.advance_to(GameTime(3 * DAY)).unwrap();
    let snapshot = rec.snapshot();
    let log = rec.log();
    assert_eq!(
        Recorder::replay(&snapshot, &log[..log.len() - 1]).unwrap_err(),
        ReplayError::LogTooShort
    );
    for cut in 0..snapshot.len() {
        assert!(
            Recorder::replay(&snapshot[..cut], log).is_err(),
            "cut {cut}"
        );
    }
    let mut padded = snapshot.clone();
    padded.push(0);
    assert_eq!(
        Recorder::replay(&padded, log).unwrap_err(),
        ReplayError::Snapshot(DecodeError::TrailingBytes)
    );
}

// Committed golden bytes (TS12) for format version 1. If a layout change breaks this test,
// bump FORMAT_VERSION, keep these bytes, and make the new build read them through a migration:
// that is the "version N loads in N+1" test from build-order stage 3.

const GOLDEN_V1_SNAPSHOT: &str = concat!(
    "010001000000080000000000000025232284e49cf2cb07a30200000000008d456cf1a90970474be4",
    "b717555ef54801c2fb03258461e087d130b21b3b1bc2020000000000000048c037401f0000000000",
    "008013000000159b0200000000009549010000000000abaaaa4a070000000000000000000000a568",
    "0100000000002517000000000000030000000000000000000000000000000a000000000000000000",
    "0080020000000000000000000000070000000600000000000000020000000000000025ba02000000",
    "000064000400000000000000000100000095ec030000000000640005000000000000000000000000",
);

const GOLDEN_V1_RECORDS: [&str; 8] = [
    "010000000000000000000001000000",
    "01000000000000000000012a00000000000000",
    "01008051010000000000041c6f6ee70f3e50bdadbdbaac1d928f02",
    "0100905f01000000000002000200000000000000",
    "0100905f0100000000000300",
    "0100905f0100000000000301",
    "0100905f010000000000030218000000",
    "010000a302000000000004f7a677f520a2e2627fcfc1a96ad1a38b",
];

fn golden_session() -> Recorder {
    let mut rec = create(42);
    rec.advance_to(GameTime(DAY + 3_600)).unwrap();
    rec.submit(Order::TransferProvince {
        province: ProvinceId(2),
        to: NationId(0),
    })
    .unwrap();
    rec.admin(AdminAction::Pause).unwrap();
    rec.admin(AdminAction::Resume).unwrap();
    rec.admin(AdminAction::SetRatio(24)).unwrap();
    rec.advance_to(GameTime(2 * DAY + 7)).unwrap();
    rec
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut s, b| {
        write!(s, "{b:02x}").unwrap();
        s
    })
}

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}

#[test]
fn format_v1_golden_bytes() {
    let rec = golden_session();
    let records: Vec<String> = rec.log().iter().map(|r| hex(&r.encode())).collect();
    assert_eq!(hex(&rec.snapshot()), GOLDEN_V1_SNAPSHOT);
    assert_eq!(records, GOLDEN_V1_RECORDS);
}

#[test]
fn format_v1_golden_snapshot_loads_and_runs_on() {
    let mut live = golden_session();
    let golden = unhex(GOLDEN_V1_SNAPSHOT);
    let mut loaded = Recorder::replay(&golden, live.log()).unwrap();
    for rec in [&mut live, &mut loaded] {
        rec.advance_to(GameTime(30 * DAY)).unwrap();
    }
    assert_eq!(loaded.snapshot(), live.snapshot());
}

#[test]
fn unknown_format_versions_are_rejected() {
    let rec = golden_session();
    let next = (FORMAT_VERSION + 1).to_le_bytes();
    let mut snapshot = rec.snapshot();
    snapshot[..2].copy_from_slice(&next);
    assert_eq!(
        Recorder::replay(&snapshot, rec.log()).unwrap_err(),
        ReplayError::Snapshot(DecodeError::UnsupportedVersion(FORMAT_VERSION + 1))
    );
    let mut record = rec.log()[0].encode();
    record[..2].copy_from_slice(&next);
    assert_eq!(
        Record::decode(&record),
        Err(DecodeError::UnsupportedVersion(FORMAT_VERSION + 1))
    );
}

fn any_entry() -> impl Strategy<Value = Entry> {
    prop_oneof![
        any::<u32>().prop_map(Entry::Ruleset),
        any::<u64>().prop_map(Entry::Seed),
        (any::<u32>(), any::<u32>()).prop_map(|(p, n)| Entry::Order(Order::TransferProvince {
            province: ProvinceId(p),
            to: NationId(n),
        })),
        Just(Entry::Admin(AdminAction::Pause)),
        Just(Entry::Admin(AdminAction::Resume)),
        any::<u32>().prop_map(|r| Entry::Admin(AdminAction::SetRatio(r))),
        (any::<u64>(), any::<u64>()).prop_map(|(state, log)| Entry::DayHash { state, log }),
    ]
}

proptest! {
    #[test]
    fn records_round_trip(at: i64, entry in any_entry()) {
        let record = Record { at: GameTime(at), entry };
        prop_assert_eq!(Record::decode(&record.encode()), Ok(record));
    }

    /// Decoding is canonical: bytes that decode at all re-encode to themselves, so the daily
    /// digest sees every corrupted byte.
    #[test]
    fn decoded_bytes_reencode_identically(
        at: i64,
        entry in any_entry(),
        pos: prop::sample::Index,
        byte: u8,
        cut in 0usize..4,
    ) {
        let mut bytes = Record { at: GameTime(at), entry }.encode();
        let i = pos.index(bytes.len());
        bytes[i] = byte;
        bytes.truncate(bytes.len() - cut);
        if let Ok(record) = Record::decode(&bytes) {
            prop_assert_eq!(record.encode(), bytes);
        }
    }
}
