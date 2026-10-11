//! Build-order stage 2 gate: a hand-made world (3 provinces, 2 nations) runs deterministically.

// `allow-unwrap-in-tests` covers `#[test]` functions only, not this file's fixture helpers.
#![allow(clippy::unwrap_used)]

use proptest::prelude::*;
use sim::fixed::Fixed;
use sim::id::NationId;
use sim::time::{GameTime, SECONDS_PER_DAY};
use sim::world::{Province, SimError, World};

const YEAR: GameTime = GameTime(365 * SECONDS_PER_DAY);

/// Committed golden value (TS12): any change to sim output fails here until updated on purpose.
const GOLDEN_HASH_SEED_42: u64 = 11_908_969_027_492_583_370;
const GOLDEN_OFFSETS_SEED_42: [i64; 2] = [84_373, 5_925];

fn tiny(seed: u64) -> World {
    let provinces = [
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
    ];
    World::new(seed, 2, &provinces).unwrap()
}

fn hash_after(seed: u64, t: GameTime) -> u64 {
    let mut w = tiny(seed);
    w.run_until(t).unwrap();
    w.state_hash()
}

fn offsets(w: &World) -> Vec<i64> {
    w.nations().iter().map(|n| n.pass_offset).collect()
}

#[test]
fn same_seed_same_hash_after_a_year_100_runs() {
    for run in 0..100 {
        assert_eq!(hash_after(42, YEAR), GOLDEN_HASH_SEED_42, "run {run}");
    }
}

#[test]
fn offsets_are_golden_distinct_and_stable() {
    let mut w = tiny(42);
    assert_eq!(offsets(&w), GOLDEN_OFFSETS_SEED_42);
    assert_ne!(w.nations()[0].pass_offset, w.nations()[1].pass_offset);
    w.run_until(YEAR).unwrap();
    assert_eq!(offsets(&w), GOLDEN_OFFSETS_SEED_42);
}

#[test]
fn each_pass_runs_at_its_offset_once_per_day() {
    let mut w = tiny(42);
    for day in 0..365 {
        w.run_until(GameTime((day + 1) * SECONDS_PER_DAY - 1))
            .unwrap();
        for n in w.nations() {
            assert_eq!(
                n.settled_at,
                GameTime(day * SECONDS_PER_DAY + n.pass_offset)
            );
        }
    }
}

#[test]
fn treasury_matches_hand_computed_income() {
    let mut w = tiny(42);
    w.run_until(YEAR).unwrap();
    // The first pass settles the partial day 0..offset; every later pass a full day, exactly.
    for (n, per_day) in w
        .nations()
        .iter()
        .zip([Fixed::from_ratio(25, 2).unwrap(), Fixed::from_int(7)])
    {
        let first = per_day.mul_ratio(n.pass_offset, SECONDS_PER_DAY).unwrap();
        let full_days = per_day.mul_ratio(364, 1).unwrap();
        assert_eq!(n.income_per_day, per_day);
        assert_eq!(n.treasury, first.checked_add(full_days).unwrap());
    }
}

#[test]
fn run_until_rejects_the_past_and_bad_owners() {
    let mut w = tiny(1);
    w.run_until(GameTime(100)).unwrap();
    assert_eq!(w.run_until(GameTime(99)), Err(SimError::TimeBeforeNow));

    let orphan = [Province {
        owner: NationId(2),
        base_income: Fixed::ONE,
    }];
    assert_eq!(
        World::new(1, 2, &orphan).err(),
        Some(SimError::UnknownNation(NationId(2)))
    );
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn different_seed_different_hash(a: u64, b: u64) {
        prop_assume!(a != b);
        prop_assert_ne!(hash_after(a, YEAR), hash_after(b, YEAR));
    }

    /// Stopping the clock at arbitrary points never changes where the world ends up.
    #[test]
    fn running_in_steps_equals_one_run(
        seed: u64,
        mut stops in prop::collection::vec(0..YEAR.0, 0..20),
    ) {
        stops.sort_unstable();
        let mut stepped = tiny(seed);
        for t in stops {
            stepped.run_until(GameTime(t)).unwrap();
        }
        stepped.run_until(YEAR).unwrap();
        prop_assert_eq!(stepped.state_hash(), hash_after(seed, YEAR));
    }

    /// Many nations force offset collisions, which must be redrawn.
    #[test]
    fn offsets_are_distinct_and_within_the_day(seed: u64) {
        let w = World::new(seed, 300, &[]).unwrap();
        let mut seen = offsets(&w);
        prop_assert!(seen.iter().all(|o| (0..SECONDS_PER_DAY).contains(o)));
        seen.sort_unstable();
        seen.dedup();
        prop_assert_eq!(seen.len(), 300);
        prop_assert_eq!(offsets(&World::new(seed, 300, &[]).unwrap()), offsets(&w));
    }
}
