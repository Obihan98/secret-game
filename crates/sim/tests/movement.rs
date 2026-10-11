//! Build-order stage 6 gate: movement on a hand-made 10-province map. Arrival times are
//! computed by hand; a speed change mid-march reschedules once; interception cases; replay
//! equality with marching armies.
//!
//! Speeds (MP7, MP3): 20 km/day on plains, forest and hills 3/4, mountains 1/2; winter and
//! foreign soil (out of supply) each another 3/4. Day 0 is 1 January 1800, so winter.

// `allow-unwrap-in-tests` covers `#[test]` functions only, not this file's fixture helpers.
#![allow(clippy::unwrap_used)]

use proptest::prelude::*;
use sim::fixed::Fixed;
use sim::id::{ArmyId, NationId, ProvinceId};
use sim::log::Recorder;
use sim::order::Order;
use sim::rng::{Rng, StreamId};
use sim::time::{GameTime, SECONDS_PER_DAY};
use sim::world::{Border, Contact, Leg, Position, Province, Scenario, SimError, Terrain, World};

const DAY: i64 = SECONDS_PER_DAY;
/// 10 April 1800: spring, so no winter slowdown unless a test wants it.
const SPRING: i64 = 100 * DAY;
const WEST: NationId = NationId(0);
const EAST: NationId = NationId(1);

/// West owns 0-4, east owns 5-9. Terrain by position in each half: plains, plains, forest,
/// hills, mountains.
///
/// ```text
///   0 --60-- 1 --30-- 2 --45-- 3 --20-- 4        west
///   |        |        |                 |
///   +--100---|--------+-> 3             |
///            40       50                80
///            |        |                 |
///            5 --60-- 6 --30-- 7 --45-- 8 --20-- 9   east (2-7, 4-9 cross)
/// ```
/// Roads (km): 0-1 60, 1-2 30, 2-3 45, 3-4 20, 0-3 100, 5-6 60, 6-7 30, 7-8 45, 8-9 20,
/// and the crossings 1-5 40, 2-7 50, 4-9 80.
fn scenario(armies: &[(NationId, u32)]) -> Scenario {
    let terrain = [
        Terrain::Plains,
        Terrain::Plains,
        Terrain::Forest,
        Terrain::Hills,
        Terrain::Mountains,
    ];
    let provinces = (0..10)
        .map(|i| Province {
            owner: if i < 5 { WEST } else { EAST },
            base_income: Fixed::ONE,
            terrain: terrain[i % 5],
        })
        .collect();
    let road = |a: u32, b: u32, km: u32| Border {
        a: ProvinceId(a),
        b: ProvinceId(b),
        length_m: km * 1_000,
    };
    Scenario {
        nations: 2,
        provinces,
        borders: vec![
            road(0, 1, 60),
            road(1, 2, 30),
            road(2, 3, 45),
            road(3, 4, 20),
            road(0, 3, 100),
            road(5, 6, 60),
            road(6, 7, 30),
            road(7, 8, 45),
            road(8, 9, 20),
            road(1, 5, 40),
            road(2, 7, 50),
            road(4, 9, 80),
        ],
        armies: armies.iter().map(|&(n, p)| (n, ProvinceId(p))).collect(),
    }
}

fn world(armies: &[(NationId, u32)]) -> World {
    World::new(42, &scenario(armies)).unwrap()
}

fn at(w: &mut World, t: i64) -> &mut World {
    w.run_until(GameTime(t)).unwrap();
    w
}

fn march(w: &mut World, army: u32, to: u32) {
    w.apply_order(Order::March {
        army: ArmyId(army),
        to: ProvinceId(to),
    })
    .unwrap();
}

fn position(w: &World, army: u32) -> Position {
    w.armies()[army as usize].position
}

const fn standing(p: u32) -> Position {
    Position::In(ProvinceId(p))
}

/// Seconds to cover `meters` at `speed` m/day, rounded up as the sim does.
const fn secs(meters: i64, speed: i64) -> i64 {
    (meters * DAY + speed - 1) / speed
}

/// Runs until one second before `t` and checks the army is still on the road to `province`,
/// then to `t`, when it must have arrived: standing there, or already on its next road.
fn arrives_exactly(w: &mut World, army: u32, t: i64, province: u32) {
    let p = ProvinceId(province);
    at(w, t - 1);
    assert!(
        matches!(position(w, army), Position::Road { leg, .. } if leg.to == p),
        "early: {:?}",
        position(w, army)
    );
    at(w, t);
    let arrived = match position(w, army) {
        Position::In(here) => here == p,
        Position::Road { from, leg } => from == p && leg.done_m == 0,
    };
    assert!(arrived, "late: {:?}", position(w, army));
}

#[test]
fn plains_at_home_in_spring_is_20_km_a_day() {
    let mut w = world(&[(WEST, 0)]);
    at(&mut w, SPRING);
    march(&mut w, 0, 1);
    // 60 km at 20 km/day: 3 days.
    arrives_exactly(&mut w, 0, SPRING + 3 * DAY, 1);
}

#[test]
fn the_fastest_route_wins_and_terrain_sets_each_leg() {
    let mut w = world(&[(WEST, 0)]);
    at(&mut w, SPRING);
    march(&mut w, 0, 4);
    // 0-3 direct (100 km into hills, 15 km/day = 6.67 days) beats 0-1-2-3 (3 + 2 + 3 days);
    // then 20 km into mountains at 10 km/day.
    assert_eq!(w.armies()[0].route, [ProvinceId(4)]);
    let into_hills = SPRING + secs(100_000, 15_000);
    arrives_exactly(&mut w, 0, into_hills, 3);
    arrives_exactly(&mut w, 0, into_hills + 2 * DAY, 4);
    assert_eq!(position(&w, 0), standing(4));
}

#[test]
fn winter_and_foreign_soil_each_cost_a_quarter() {
    // Day 0 is winter: 60 km of home plains at 15 km/day = 4 days.
    let mut w = world(&[(WEST, 0)]);
    march(&mut w, 0, 1);
    arrives_exactly(&mut w, 0, 4 * DAY, 1);

    // Spring, into foreign plains: 40 km at 15 km/day.
    let mut w = world(&[(WEST, 1)]);
    at(&mut w, SPRING);
    march(&mut w, 0, 5);
    arrives_exactly(&mut w, 0, SPRING + secs(40_000, 15_000), 5);

    // Winter and foreign forest: 20 * 3/4 * 3/4 * 3/4 = 8.437 km/day (each step rounds down).
    let mut w = world(&[(WEST, 2)]);
    march(&mut w, 0, 7);
    arrives_exactly(&mut w, 0, secs(50_000, 8_437), 7);
}

#[test]
fn a_speed_change_mid_march_reschedules_once() {
    let mut w = world(&[(WEST, 0)]);
    let offset = w.nations()[0].pass_offset;
    // Leave on day 57 (winter): 100 km into hills at 15 * 3/4 = 11.25 km/day.
    let start = 57 * DAY;
    at(&mut w, start);
    march(&mut w, 0, 3);
    let winter_arrival = start + secs(100_000, 11_250);

    // Day 58's pass: still winter, nothing to reschedule (only the pass itself re-queues).
    at(&mut w, 58 * DAY + offset - 1);
    let before = w.events_scheduled();
    at(&mut w, 58 * DAY + offset);
    assert_eq!(w.events_scheduled(), before + 1);

    // Day 59 (1 March) is spring: west's pass speeds the army up to 15 km/day, once.
    let pass = 59 * DAY + offset;
    at(&mut w, pass - 1);
    let before = w.events_scheduled();
    at(&mut w, pass);
    assert_eq!(
        w.events_scheduled(),
        before + 2,
        "the pass plus one new arrival"
    );
    let done = 11_250 * (pass - start) / DAY;
    let Position::Road { leg, .. } = position(&w, 0) else {
        panic!("not on the road");
    };
    assert_eq!(
        leg,
        Leg {
            to: ProvinceId(3),
            length_m: 100_000,
            done_m: u32::try_from(done).unwrap(),
            settled_at: GameTime(pass),
            speed: 15_000,
        }
    );
    let spring_arrival = pass + secs(100_000 - done, 15_000);
    assert!(spring_arrival < winter_arrival);
    arrives_exactly(&mut w, 0, spring_arrival, 3);

    // The stale winter arrival changes nothing when its time comes.
    let generation = w.armies()[0].generation;
    at(&mut w, winter_arrival + 1);
    assert_eq!(position(&w, 0), standing(3));
    assert_eq!(w.armies()[0].generation, generation);
}

#[test]
fn head_on_armies_halt_where_they_meet() {
    // West at 1, east at 5, both onto the 40 km road, each at 15 km/day on foreign soil.
    let mut w = world(&[(WEST, 1), (EAST, 5)]);
    at(&mut w, SPRING);
    march(&mut w, 0, 5);
    march(&mut w, 1, 1);
    let meet = SPRING + secs(40_000, 30_000);
    at(&mut w, meet - 1);
    assert_eq!(w.contacts(), []);
    at(&mut w, meet);
    // The east army set out last, so its plan found the meeting: it is side `a`.
    assert_eq!(
        w.contacts(),
        [Contact {
            at: GameTime(meet),
            a: ArmyId(1),
            b: ArmyId(0),
        }]
    );
    for army in [0, 1] {
        let Position::Road { leg, .. } = position(&w, army) else {
            panic!("left the road");
        };
        assert_eq!((leg.done_m, leg.speed), (20_000, 0));
    }
    // Their old arrivals are stale: a week later both still stand on the road.
    at(&mut w, meet + 7 * DAY);
    assert_eq!(w.contacts().len(), 1);
    assert!(matches!(position(&w, 0), Position::Road { .. }));
}

#[test]
fn armies_of_one_nation_pass_each_other() {
    let mut w = world(&[(WEST, 1), (WEST, 5)]);
    at(&mut w, SPRING);
    march(&mut w, 0, 5);
    march(&mut w, 1, 1);
    at(&mut w, SPRING + 5 * DAY);
    assert_eq!(w.contacts(), []);
    assert_eq!(position(&w, 0), standing(5));
    assert_eq!(position(&w, 1), standing(1));
}

#[test]
fn a_marching_army_walks_into_a_halted_one() {
    // As above, plus a second west army that reaches 1 three days later and takes the road.
    let mut w = world(&[(WEST, 1), (EAST, 5), (WEST, 0)]);
    at(&mut w, SPRING);
    march(&mut w, 0, 5);
    march(&mut w, 1, 1);
    march(&mut w, 2, 5);
    // Reaches 1 after 60 km at 20 km/day, then 20 km to the halted east army at 15 km/day.
    let meet = SPRING + 3 * DAY + secs(20_000, 15_000);
    at(&mut w, meet);
    assert_eq!(w.contacts().len(), 2);
    assert_eq!(
        w.contacts()[1],
        Contact {
            at: GameTime(meet),
            a: ArmyId(2),
            b: ArmyId(1),
        }
    );
}

#[test]
fn arriving_where_an_enemy_stands_stops_the_march() {
    let mut w = world(&[(WEST, 5), (EAST, 6)]);
    at(&mut w, SPRING);
    // Heading for 7 through 6, where the east army stands.
    march(&mut w, 0, 7);
    let arrival = SPRING + secs(60_000, 15_000);
    arrives_exactly(&mut w, 0, arrival, 6);
    assert_eq!(
        w.contacts(),
        [Contact {
            at: GameTime(arrival),
            a: ArmyId(0),
            b: ArmyId(1),
        }]
    );
    assert_eq!(w.armies()[0].route, []);
    at(&mut w, arrival + 10 * DAY);
    assert_eq!(position(&w, 0), standing(6));
}

#[test]
fn a_faster_army_catches_up_on_the_same_road() {
    // Both from 6 into 7 (east forest): west at 11.25 km/day (foreign), east at 15 km/day,
    // half a day later. Gap 5.625 km, closing 3.75 km/day: 1.5 days, before west arrives.
    let mut w = world(&[(WEST, 6), (EAST, 6)]);
    at(&mut w, SPRING);
    march(&mut w, 0, 7);
    at(&mut w, SPRING + DAY / 2);
    march(&mut w, 1, 7);
    let meet = SPRING + 2 * DAY;
    assert!(meet < SPRING + secs(30_000, 11_250));
    at(&mut w, meet - 1);
    assert_eq!(w.contacts(), []);
    at(&mut w, meet);
    assert_eq!(w.contacts().len(), 1);
}

#[test]
fn a_replanned_meeting_happens_once_at_the_new_time() {
    let mut w = world(&[(WEST, 1), (EAST, 5)]);
    let offset = w.nations()[0].pass_offset;
    let start = SPRING + offset - 1_000;
    at(&mut w, start);
    march(&mut w, 0, 5);
    march(&mut w, 1, 1);
    // Province 5 changes hands: the west army is now marching home, and west's pass 1,000 s
    // later speeds it up to 20 km/day; the meeting moves earlier.
    w.apply_order(Order::TransferProvince {
        province: ProvinceId(5),
        to: WEST,
    })
    .unwrap();
    let pass = start + 1_000;
    let moved = 15_000 * 1_000 / DAY;
    let meet = pass + secs(40_000 - 2 * moved, 35_000);
    assert!(meet < start + secs(40_000, 30_000));
    at(&mut w, meet - 1);
    assert_eq!(w.contacts(), []);
    at(&mut w, start + 3 * DAY);
    assert_eq!(w.contacts().len(), 1);
    assert_eq!(w.contacts()[0].at, GameTime(meet));
}

#[test]
fn a_halted_army_can_turn_back() {
    let mut w = world(&[(WEST, 1), (EAST, 5)]);
    at(&mut w, SPRING);
    march(&mut w, 0, 5);
    march(&mut w, 1, 1);
    let meet = SPRING + secs(40_000, 30_000);
    at(&mut w, meet);
    // Home to 0: back 20 km to 1 at 20 km/day (1 day), then 60 km (3 days). Walking away
    // from the east army is no new contact.
    march(&mut w, 0, 0);
    arrives_exactly(&mut w, 0, meet + DAY, 1);
    arrives_exactly(&mut w, 0, meet + 4 * DAY, 0);
    assert_eq!(position(&w, 0), standing(0));
    assert_eq!(w.contacts().len(), 1);
}

#[test]
fn bad_march_orders_change_nothing() {
    let mut scenario = scenario(&[(WEST, 0)]);
    scenario.provinces.push(Province {
        owner: WEST,
        base_income: Fixed::ONE,
        terrain: Terrain::Plains,
    });
    let mut w = World::new(1, &scenario).unwrap();
    let hash = w.state_hash();
    let cases = [
        ((5, 1), SimError::UnknownArmy(ArmyId(5))),
        ((0, 99), SimError::UnknownProvince(ProvinceId(99))),
        ((0, 0), SimError::NoChange),
        ((0, 10), SimError::Unreachable),
    ];
    for ((army, to), err) in cases {
        let order = Order::March {
            army: ArmyId(army),
            to: ProvinceId(to),
        };
        assert_eq!(w.apply_order(order), Err(err));
    }
    assert_eq!(w.state_hash(), hash);
}

#[test]
fn bad_scenarios_are_rejected() {
    let mut s = scenario(&[]);
    s.borders.push(Border {
        a: ProvinceId(0),
        b: ProvinceId(1),
        length_m: 5,
    });
    assert_eq!(World::new(1, &s).err(), Some(SimError::BadBorder(12)));
    let s = scenario(&[(NationId(2), 0)]);
    assert_eq!(
        World::new(1, &s).err(),
        Some(SimError::UnknownNation(NationId(2)))
    );
    let s = scenario(&[(WEST, 10)]);
    assert_eq!(
        World::new(1, &s).err(),
        Some(SimError::UnknownProvince(ProvinceId(10)))
    );
}

/// Random marches and transfers by a script with its own randomness, recorded live.
fn campaign(seed: u64, days: i64, orders: usize) -> (Recorder, Vec<u8>, Vec<(i64, usize, u64)>) {
    let armies: Vec<_> = (0..8).map(|i| (NationId(i % 2), i * 9 % 10)).collect();
    let mut rec = Recorder::create(seed, &scenario(&armies)).unwrap();
    let start = rec.snapshot();
    let mut rng = Rng::new(seed, StreamId(1_000));
    let mut pick = |n: u64| rng.next_u64() % n;
    let mut times: Vec<i64> = (0..orders)
        .map(|_| i64::try_from(pick(u64::try_from(days * DAY).unwrap())).unwrap())
        .collect();
    times.sort_unstable();
    let mut checkpoints = Vec::new();
    for t in times {
        rec.advance_to(GameTime(t)).unwrap();
        let order = if pick(5) == 0 {
            Order::TransferProvince {
                province: ProvinceId(u32::try_from(pick(10)).unwrap()),
                to: NationId(u32::try_from(pick(2)).unwrap()),
            }
        } else {
            Order::March {
                army: ArmyId(u32::try_from(pick(8)).unwrap()),
                to: ProvinceId(u32::try_from(pick(10)).unwrap()),
            }
        };
        let result = rec.submit(order);
        assert!(
            matches!(result, Ok(()) | Err(SimError::NoChange)),
            "{result:?}"
        );
        // An order can schedule an event for this very second (two armies meeting on the
        // spot); it runs on the next step. Take that step so the checkpoint is comparable.
        rec.advance_to(GameTime(t)).unwrap();
        checkpoints.push((t, rec.log().len(), rec.world().state_hash()));
    }
    (rec, start, checkpoints)
}

#[test]
fn replay_with_marching_armies_equals_live() {
    let (live, start, checkpoints) = campaign(42, 365, 400);
    assert!(
        live.world().contacts().len() > 5,
        "the campaign should see contacts"
    );
    for &(t, len, hash) in &checkpoints {
        let mut replayed = Recorder::replay(&start, &live.log()[..len]).unwrap();
        replayed.advance_to(GameTime(t)).unwrap();
        assert_eq!(replayed.world().state_hash(), hash, "t={t}");
    }
}

#[test]
fn snapshots_mid_march_resume_identically() {
    let (mut live, _, _) = campaign(7, 30, 60);
    assert!(
        live.world()
            .armies()
            .iter()
            .any(|a| matches!(a.position, Position::Road { .. })),
        "some army should be on a road"
    );
    let snapshot = live.snapshot();
    let mut resumed = Recorder::replay(&snapshot, live.log()).unwrap();
    for rec in [&mut live, &mut resumed] {
        rec.advance_to(GameTime(200 * DAY)).unwrap();
    }
    assert_eq!(resumed.snapshot(), live.snapshot());
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(16))]

    #[test]
    fn same_seed_and_orders_same_hash(seed: u64) {
        let (a, _, _) = campaign(seed, 120, 150);
        let (b, _, _) = campaign(seed, 120, 150);
        prop_assert_eq!(a.world().state_hash(), b.world().state_hash());
    }
}
