//! Build-order stage 4: the scale spike (TS13, T8). Results go into TS13 in `docs/tech/stack.md`.
//!
//! Gate: one full game-day of the synthetic world (every nation's daily pass plus the daily
//! hash) costs at most 1 s of single-thread time. Run with `cargo bench -p sim`.
//!
//! The stage 4 benches keep their original world (random owners, no map) so later stages are
//! compared like for like; stage 6 adds a grid map with marching armies.

// Bench fixtures may unwrap: a broken fixture should stop the run loudly.
#![allow(clippy::unwrap_used)]

use std::hint::black_box;

use criterion::{BatchSize, Criterion, Throughput, criterion_group, criterion_main};
use sim::fixed::Fixed;
use sim::id::{ArmyId, NationId, ProvinceId};
use sim::log::Recorder;
use sim::order::Order;
use sim::queue::{EventQueue, Priority};
use sim::rng::{Rng, StreamId};
use sim::time::{GameTime, SECONDS_PER_DAY};
use sim::world::{Border, Province, Scenario, Terrain, World};

const PROVINCES: u32 = 30_000;
const NATIONS: u32 = 200;
const CLIENTS: u32 = 200;
/// Provinces a nation sees beyond its own: a stand-in for borders and spies until fog of war
/// (MP9) is designed.
const EXTRA_VISIBLE: u32 = 300;
/// The stage 6 grid map: 200 x 150 provinces, nations owning 20 x 15 blocks of them.
const GRID_W: u32 = 200;
const GRID_H: u32 = 150;
const ARMIES: u32 = 2_000;

/// The bench's own randomness, from a stream no sim system uses.
fn bench_rng() -> Rng {
    Rng::new(2026, StreamId(1_000))
}

fn below(rng: &mut Rng, n: u32) -> u32 {
    u32::try_from(rng.next_u64() % u64::from(n)).unwrap()
}

/// 30,000 provinces with random owners among 200 nations and incomes of 0.00 to 9.99 a day.
fn synthetic_provinces() -> Vec<Province> {
    let mut rng = bench_rng();
    (0..PROVINCES)
        .map(|_| Province {
            owner: NationId(below(&mut rng, NATIONS)),
            base_income: Fixed::from_ratio(i64::from(below(&mut rng, 1_000)), 100).unwrap(),
            terrain: Terrain::Plains,
        })
        .collect()
}

fn stage4_world() -> Scenario {
    Scenario {
        nations: NATIONS,
        provinces: synthetic_provinces(),
        ..Scenario::default()
    }
}

/// 30,000 provinces on a grid, each bordering its four neighbours by roads of 20 to 60 km;
/// mixed terrain; each nation owns a 20 x 15 block; armies stand in random provinces of
/// their owners.
fn grid_world() -> Scenario {
    let mut rng = bench_rng();
    let terrains = [
        Terrain::Plains,
        Terrain::Plains,
        Terrain::Forest,
        Terrain::Hills,
    ];
    let id = |x: u32, y: u32| ProvinceId(y * GRID_W + x);
    let mut provinces = Vec::new();
    let mut borders = Vec::new();
    for y in 0..GRID_H {
        for x in 0..GRID_W {
            let terrain = if below(&mut rng, 20) == 0 {
                Terrain::Mountains
            } else {
                terrains[below(&mut rng, 4) as usize]
            };
            provinces.push(Province {
                owner: NationId((y / 15) * (GRID_W / 20) + x / 20),
                base_income: Fixed::from_int(1),
                terrain,
            });
            for (nx, ny) in [(x + 1, y), (x, y + 1)] {
                if nx < GRID_W && ny < GRID_H {
                    borders.push(Border {
                        a: id(x, y),
                        b: id(nx, ny),
                        length_m: 20_000 + below(&mut rng, 40_000),
                    });
                }
            }
        }
    }
    let armies = (0..ARMIES)
        .map(|_| {
            let p = below(&mut rng, PROVINCES);
            (provinces[p as usize].owner, ProvinceId(p))
        })
        .collect();
    Scenario {
        nations: NATIONS,
        provinces,
        borders,
        armies,
    }
}

/// The grid world one game-day into a campaign: every army marching to a province up to 15
/// steps away in each direction, so some cross borders and meet.
fn marching_world() -> World {
    let scenario = grid_world();
    let mut world = World::new(1, &scenario).unwrap();
    let mut rng = bench_rng();
    for (i, &(_, at)) in (0..).zip(&scenario.armies) {
        let (x, y) = (at.0 % GRID_W, at.0 / GRID_W);
        let dx = (x + below(&mut rng, 31)).saturating_sub(15).min(GRID_W - 1);
        let dy = (y + below(&mut rng, 31)).saturating_sub(15).min(GRID_H - 1);
        let to = ProvinceId(dy * GRID_W + dx);
        if to != at {
            world
                .apply_order(Order::March {
                    army: ArmyId(i),
                    to,
                })
                .unwrap();
        }
    }
    world.run_until(GameTime(SECONDS_PER_DAY)).unwrap();
    world
}

fn game_day(c: &mut Criterion) {
    let scenario = stage4_world();
    let mut g = c.benchmark_group("game_day");

    // Every nation's daily pass, nothing else.
    let mut world = World::new(1, &scenario).unwrap();
    let mut t = 0;
    g.bench_function("daily_passes", |b| {
        b.iter(|| {
            t += SECONDS_PER_DAY;
            world.run_until(GameTime(t)).unwrap();
        });
    });

    // What the live server does per game-day: the passes plus the daily hash record. The gate.
    let mut rec = Recorder::create(1, &scenario).unwrap();
    let mut t = 0;
    g.bench_function("passes_and_daily_hash", |b| {
        b.iter(|| {
            t += SECONDS_PER_DAY;
            rec.advance_to(GameTime(t)).unwrap();
        });
    });

    // Stage 6: the same day on the grid map with 2,000 armies marching (arrivals, speed
    // changes, meetings), always from the same starting state.
    let marching = marching_world();
    let next_day = GameTime(2 * SECONDS_PER_DAY);
    g.bench_function("passes_2000_armies_marching", |b| {
        b.iter_batched(
            || marching.clone(),
            |mut w| w.run_until(next_day).unwrap(),
            BatchSize::LargeInput,
        );
    });
    g.bench_function("state_hash_2000_armies", |b| {
        b.iter(|| black_box(marching.state_hash()));
    });
    g.finish();
}

/// Steady state of a busy queue: pop the earliest event, schedule one later, at several queue
/// sizes. The cost every event pays, whatever it does.
fn event_queue(c: &mut Criterion) {
    let mut g = c.benchmark_group("event_queue");
    g.throughput(Throughput::Elements(1));
    for pending in [1_000u32, 100_000, 1_000_000] {
        let mut rng = bench_rng();
        let mut q = EventQueue::new();
        for i in 0..pending {
            q.push(
                GameTime(i64::from(below(&mut rng, 86_400))),
                Priority(100),
                i,
            );
        }
        g.bench_function(format!("pop_push_{pending}_pending"), |b| {
            b.iter(|| {
                let (key, e) = q.pop().unwrap();
                let later = i64::from(below(&mut rng, 86_400)) + 1;
                q.push(GameTime(key.time.0 + later), key.priority, black_box(e));
            });
        });
    }
    g.finish();
}

fn snapshot(c: &mut Criterion) {
    let mut rec = Recorder::create(1, &stage4_world()).unwrap();
    rec.advance_to(GameTime(30 * SECONDS_PER_DAY)).unwrap();
    let bytes = rec.snapshot();
    println!("snapshot size: {} bytes", bytes.len());

    let mut g = c.benchmark_group("snapshot");
    g.throughput(Throughput::Bytes(bytes.len() as u64));
    g.bench_function("state_hash", |b| {
        b.iter(|| black_box(rec.world().state_hash()));
    });
    g.bench_function("save", |b| b.iter(|| black_box(rec.snapshot())));
    g.bench_function("load", |b| {
        b.iter(|| black_box(Recorder::replay(&bytes, rec.log()).unwrap()));
    });
    g.finish();
}

/// Deciding which of 200 clients may see an event about one province. Each client plays one
/// nation and sees its own provinces plus some others (bitset per nation). Stub only: fog of
/// war (MP9) and networking (TS7) are not designed yet; this measures the filtering cost.
fn fan_out(c: &mut Criterion) {
    let provinces = synthetic_provinces();
    let words = PROVINCES.div_ceil(64) as usize;
    let mut rng = bench_rng();
    let mut visible = vec![vec![0u64; words]; NATIONS as usize];
    let mut see = |nation: u32, p: u32| {
        visible[nation as usize][(p / 64) as usize] |= 1 << (p % 64);
    };
    for (p, prov) in (0..).zip(&provinces) {
        see(prov.owner.0, p);
    }
    for nation in 0..NATIONS {
        for _ in 0..EXTRA_VISIBLE {
            see(nation, below(&mut rng, PROVINCES));
        }
    }
    let clients: Vec<u32> = (0..CLIENTS).map(|i| i % NATIONS).collect();
    let events: Vec<u32> = (0..10_000).map(|_| below(&mut rng, PROVINCES)).collect();

    let mut g = c.benchmark_group("fan_out");
    g.throughput(Throughput::Elements(events.len() as u64));
    g.bench_function("10k_events_200_clients", |b| {
        b.iter_batched_ref(
            Vec::new,
            |recipients: &mut Vec<u32>| {
                for &p in &events {
                    recipients.clear();
                    let (word, bit) = ((p / 64) as usize, p % 64);
                    recipients.extend(
                        clients
                            .iter()
                            .copied()
                            .filter(|&n| visible[n as usize][word] >> bit & 1 == 1),
                    );
                    black_box(&recipients);
                }
            },
            BatchSize::SmallInput,
        );
    });
    g.finish();
}

criterion_group!(benches, game_day, event_queue, snapshot, fan_out);
criterion_main!(benches);
