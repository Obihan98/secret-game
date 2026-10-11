//! Build-order stage 5 gate: persistence. A world killed at random points recovers to the
//! clean run; nothing is acked before it is durable; a backup restores correctly.

// `allow-unwrap-in-tests` covers `#[test]` functions only, not this file's fixture helpers.
#![allow(clippy::unwrap_used)]

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

use rusqlite::Connection;
use server::host::{Ack, Host};
use server::store::StoreError;
use sim::fixed::Fixed;
use sim::id::{NationId, ProvinceId};
use sim::log::{AdminAction, Entry, Record, Recorder};
use sim::order::Order;
use sim::rng::{Rng, StreamId};
use sim::time::{GameTime, SECONDS_PER_DAY};
use sim::world::{Province, SimError};

const SEED: u64 = 42;
const STEPS: usize = 10_000;
/// 15 game-minutes per step: the script spans about 100 game-days.
const STEP_SECONDS: i64 = 900;
const FLUSH_EVERY: usize = 5;
const SNAPSHOT_EVERY: usize = 700;
const KILLS: usize = 8;
const WORKER_ENV: &str = "CRASH_WORKER_DB";

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

/// A fresh, empty directory under Cargo's per-test scratch space.
fn fresh_dir(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[derive(Clone, Copy, Debug)]
enum Op {
    Wait,
    Order(Order),
    Admin(AdminAction),
}

/// The same scripted session every time; its randomness comes from a stream no sim system uses.
fn script() -> Vec<Op> {
    let mut rng = Rng::new(99, StreamId(1_000));
    let mut pick = |n: u64| u32::try_from(rng.next_u64() % n).unwrap();
    (0..STEPS)
        .map(|_| match pick(10) {
            0..=3 => Op::Order(Order::TransferProvince {
                province: ProvinceId(pick(3)),
                to: NationId(pick(2)),
            }),
            4 => Op::Admin(match pick(3) {
                0 => AdminAction::Pause,
                1 => AdminAction::Resume,
                _ => AdminAction::SetRatio(pick(48) + 1),
            }),
            _ => Op::Wait,
        })
        .collect()
}

fn step_time(i: usize) -> GameTime {
    GameTime(i64::try_from(i + 1).unwrap() * STEP_SECONDS)
}

const fn is_snapshot_step(i: usize) -> bool {
    i % SNAPSHOT_EVERY == SNAPSHOT_EVERY - 1
}

const fn is_flush_step(i: usize) -> bool {
    i % FLUSH_EVERY == FLUSH_EVERY - 1
}

/// A random transfer may hand a province to its owner; that is rejected, not logged.
fn accepted(result: Result<(), SimError>) -> bool {
    assert!(
        matches!(result, Ok(()) | Err(SimError::NoChange)),
        "{result:?}"
    );
    result.is_ok()
}

/// The reference: the whole script in memory, no file. Also the log length after each step.
fn clean_run() -> (Recorder, Vec<usize>) {
    let mut rec = Recorder::create(SEED, 2, &provinces()).unwrap();
    let mut lens = Vec::new();
    for (i, op) in script().into_iter().enumerate() {
        rec.advance_to(step_time(i)).unwrap();
        match op {
            Op::Wait => {}
            Op::Order(o) => {
                accepted(rec.submit(o));
            }
            Op::Admin(a) => rec.admin(a).unwrap(),
        }
        lens.push(rec.log().len());
    }
    (rec, lens)
}

/// One scripted step on a hosted world: the clean run's step, plus flushes and snapshots.
fn host_step(host: &mut Host, i: usize, op: Op) -> Vec<Ack> {
    host.advance_to(step_time(i)).unwrap();
    match op {
        Op::Wait => {}
        Op::Order(o) => {
            accepted(host.submit(o));
        }
        Op::Admin(a) => host.admin(a).unwrap(),
    }
    if is_snapshot_step(i) {
        host.snapshot().unwrap()
    } else if is_flush_step(i) {
        host.flush().unwrap()
    } else {
        Vec::new()
    }
}

/// The first step a recovered world with `log_len` records still has to run. Steps that log
/// nothing can be skipped or re-run alike: running in steps equals one run (stage 2).
fn resume_step(lens: &[usize], initial_len: usize, log_len: usize) -> usize {
    let start = lens.partition_point(|&l| l <= log_len);
    let before = start.checked_sub(1).map_or(initial_len, |i| lens[i]);
    assert_eq!(before, log_len, "recovered log ends between steps");
    start
}

fn initial_len() -> usize {
    Recorder::create(SEED, 2, &provinces()).unwrap().log().len()
}

/// The crash test's child process: opens (or creates) the world, runs the rest of the script
/// and prints every ack the moment it is released. The parent kills it at a random point.
#[test]
#[ignore = "child process of crash_recovers_to_the_clean_run"]
fn crash_worker() {
    let Some(db) = std::env::var_os(WORKER_ENV).map(PathBuf::from) else {
        return;
    };
    let (_, lens) = clean_run();
    let (mut host, start) = if db.exists() {
        let host = Host::open(&db).unwrap();
        let start = resume_step(&lens, initial_len(), host.recorder().log().len());
        (host, start)
    } else {
        (Host::create(&db, SEED, 2, &provinces()).unwrap(), 0)
    };
    let mut out = std::io::stdout().lock();
    for (i, op) in script().into_iter().enumerate().skip(start) {
        for ack in host_step(&mut host, i, op) {
            writeln!(out, "ack {}", ack.log_index).unwrap();
            out.flush().unwrap();
        }
    }
    for ack in host.flush().unwrap() {
        writeln!(out, "ack {}", ack.log_index).unwrap();
    }
    writeln!(out, "done").unwrap();
}

/// Runs `crash_worker` in a child process; the returned thread collects its stdout.
fn spawn_worker(db: &Path) -> (std::process::Child, thread::JoinHandle<Vec<String>>) {
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["crash_worker", "--exact", "--ignored", "--nocapture"])
        .args(["--test-threads", "1"])
        .env(WORKER_ENV, db)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let stdout = child.stdout.take().unwrap();
    // A reader thread, so a full pipe never blocks the child while we wait to kill it.
    let reader = thread::spawn(move || {
        BufReader::new(stdout)
            .lines()
            .map_while(Result::ok)
            .collect()
    });
    (child, reader)
}

fn acked(lines: &[String]) -> Vec<usize> {
    lines
        .iter()
        .filter_map(|l| l.strip_prefix("ack "))
        .map(|n| n.parse().unwrap())
        .collect()
}

#[test]
fn crash_recovers_to_the_clean_run() {
    let db = fresh_dir("crash").join("world.db");
    let (mut clean, lens) = clean_run();
    let initial = initial_len();
    let mut durable_points: Vec<usize> = (0..STEPS)
        .filter(|&i| is_flush_step(i) || is_snapshot_step(i))
        .map(|i| lens[i])
        .collect();
    durable_points.push(initial);

    let mut rng = Rng::new(5, StreamId(1_001));
    let mut interrupted = 0;
    let mut last_len = 0;
    for kill in 0..KILLS {
        let (mut child, reader) = spawn_worker(&db);
        thread::sleep(Duration::from_millis(30 + rng.next_u64() % 300));
        if child.try_wait().unwrap().is_none() {
            interrupted += 1;
        }
        child.kill().unwrap();
        child.wait().unwrap();
        let lines = reader.join().unwrap();

        // Like a server restart: recover from the file alone.
        let host = Host::open(&db).unwrap();
        let log = host.recorder().log();
        assert_eq!(log, &clean.log()[..log.len()], "kill {kill}");
        assert!(
            durable_points.contains(&log.len()),
            "kill {kill}: torn flush"
        );
        assert!(log.len() >= last_len, "kill {kill}: lost committed records");
        // Every ack a client could have seen is in the recovered log.
        assert!(acked(&lines).iter().all(|&i| i < log.len()), "kill {kill}");
        last_len = log.len();
    }
    assert!(
        interrupted >= KILLS / 2,
        "only {interrupted} kills hit a running world"
    );

    let (mut child, reader) = spawn_worker(&db);
    assert!(child.wait().unwrap().success());
    assert!(reader.join().unwrap().iter().any(|l| l == "done"));

    let mut host = Host::open(&db).unwrap();
    assert_eq!(host.recorder().log(), clean.log());
    let end = GameTime(step_time(STEPS).0 + SECONDS_PER_DAY);
    host.advance_to(end).unwrap();
    clean.advance_to(end).unwrap();
    assert_eq!(host.recorder().snapshot(), clean.snapshot());
}

#[test]
fn nothing_is_acked_before_it_is_durable() {
    let db = fresh_dir("ordering").join("world.db");
    let mut host = Host::create(&db, SEED, 2, &provinces()).unwrap();
    // A second connection sees only committed transactions, as a recovering server would.
    let reader = Connection::open(&db).unwrap();
    let stored = |seq: usize| -> Option<Record> {
        reader
            .query_row(
                "SELECT record FROM log WHERE seq = ?1",
                [i64::try_from(seq).unwrap()],
                |r| r.get::<_, Vec<u8>>(0),
            )
            .ok()
            .map(|bytes| Record::decode(&bytes).unwrap())
    };

    let mut acks = Vec::new();
    for (i, op) in script().into_iter().enumerate().take(600) {
        let before = host.recorder().log().len();
        let released = host_step(&mut host, i, op);
        if released.is_empty() {
            // Unflushed records are not in the file yet: an ack for them would be premature.
            for seq in host.durable_len()..host.recorder().log().len() {
                assert_eq!(
                    stored(seq),
                    None,
                    "step {i}: record {seq} visible before commit"
                );
            }
        }
        for ack in released {
            let record = stored(ack.log_index).expect("acked record is not durable");
            assert_eq!(record, host.recorder().log()[ack.log_index]);
            assert!(matches!(record.entry, Entry::Order(_)));
            acks.push(ack.log_index);
        }
        assert!(host.recorder().log().len() >= before);
    }
    host.flush()
        .unwrap()
        .into_iter()
        .for_each(|a| acks.push(a.log_index));

    // Every accepted order is acked exactly once, in log order.
    let orders: Vec<usize> = (0..host.recorder().log().len())
        .filter(|&i| matches!(host.recorder().log()[i].entry, Entry::Order(_)))
        .collect();
    assert!(orders.len() > 100);
    assert_eq!(acks, orders);
}

#[test]
fn backup_restores_to_the_moment_it_was_taken() {
    let dir = fresh_dir("backup");
    let (db, copy) = (dir.join("world.db"), dir.join("backup.db"));
    let (mut clean, lens) = clean_run();
    let mut host = Host::create(&db, SEED, 2, &provinces()).unwrap();
    let ops = script();
    let half = STEPS / 2;
    for (i, &op) in ops.iter().enumerate().take(half) {
        host_step(&mut host, i, op);
    }
    // Taken mid-session, with records pending: the copy holds what is committed.
    host.backup_to(&copy).unwrap();
    let durable = host.durable_len();
    for (i, &op) in ops.iter().enumerate().skip(half) {
        host_step(&mut host, i, op);
    }
    drop(host);

    let mut restored = Host::open(&copy).unwrap();
    assert_eq!(restored.recorder().log(), &clean.log()[..durable]);
    let start = resume_step(&lens, initial_len(), durable);
    for (i, &op) in ops.iter().enumerate().skip(start) {
        host_step(&mut restored, i, op);
    }
    let end = step_time(STEPS);
    restored.advance_to(end).unwrap();
    clean.advance_to(end).unwrap();
    assert_eq!(restored.recorder().snapshot(), clean.snapshot());
}

#[test]
fn files_are_wal_and_refuse_misuse() {
    let dir = fresh_dir("misuse");
    let db = dir.join("world.db");
    drop(Host::create(&db, SEED, 2, &provinces()).unwrap());

    let raw = Connection::open(&db).unwrap();
    let mode: String = raw
        .pragma_query_value(None, "journal_mode", |r| r.get(0))
        .unwrap();
    assert_eq!(mode, "wal");

    assert!(matches!(
        Host::create(&db, SEED, 2, &provinces()),
        Err(StoreError::Exists)
    ));
    assert!(matches!(
        Host::open(&dir.join("missing.db")),
        Err(StoreError::Sqlite(_))
    ));
    raw.pragma_update(None, "user_version", 99).unwrap();
    assert!(matches!(Host::open(&db), Err(StoreError::Schema(99))));
}
