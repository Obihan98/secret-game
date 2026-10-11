//! The input log, snapshots and replay (TM9, TS6).
//!
//! The log holds inputs only (ruleset stamps, seeds, orders, admin actions) plus one
//! `DayHash` record per game-day; replay re-derives every other event by running the sim.
//! `Recorder` is the one code path for both: the live server drives it with inputs, and
//! `Recorder::replay` feeds it the logged ones, checking each daily hash on the way.

use std::fmt;

use crate::codec::{DecodeError, Reader, Sink, Writer};
use crate::hash::StateHasher;
use crate::order::Order;
use crate::time::{GameTime, SECONDS_PER_DAY};
use crate::world::{Scenario, SimError, World};

/// The rules this build runs (TM9). Bumped by every game update that can change outcomes;
/// replay refuses snapshots and log records from any other version.
pub const RULESET_VERSION: u32 = 1;

/// The byte layout of log records (TS6). Bumped on any layout change, together with a
/// migration that reads the old version. New tags for new kinds of entries or orders extend
/// the layout without changing it: old records keep their exact bytes.
pub const RECORD_FORMAT: u16 = 1;

/// The byte layout of snapshots (TS6). Version 2 (build-order stage 6) adds terrain, borders,
/// armies and contacts; version 1 snapshots still load (`World::decode`).
pub const SNAPSHOT_FORMAT: u16 = 2;

/// Admin actions only steer the server's real-time clock; they change no sim state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdminAction {
    Pause,
    Resume,
    /// Game-seconds per real second (TM1; 24 by default).
    SetRatio(u32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Entry {
    /// From here on the world runs this ruleset version: written at creation and by every game
    /// update (TM9).
    Ruleset(u32),
    /// The world seed every RNG stream derives from (TM9).
    Seed(u64),
    Order(Order),
    Admin(AdminAction),
    /// Written at each game-day boundary: the state hash, and a digest of the bytes of every
    /// record since the previous `DayHash`. The digest catches a corrupted record that changes
    /// no state (an admin action, a seed), which the state hash alone would miss.
    DayHash {
        state: u64,
        log: u64,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Record {
    pub at: GameTime,
    pub entry: Entry,
}

impl Record {
    /// Every record carries the format version (TS6). Tags are part of the format: never reuse
    /// or renumber one.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.put_u16(RECORD_FORMAT);
        w.put_i64(self.at.seconds());
        match self.entry {
            Entry::Ruleset(version) => {
                w.put_u8(0);
                w.put_u32(version);
            }
            Entry::Seed(seed) => {
                w.put_u8(1);
                w.put_u64(seed);
            }
            Entry::Order(order) => {
                w.put_u8(2);
                order.encode(&mut w);
            }
            Entry::Admin(action) => {
                w.put_u8(3);
                match action {
                    AdminAction::Pause => w.put_u8(0),
                    AdminAction::Resume => w.put_u8(1),
                    AdminAction::SetRatio(ratio) => {
                        w.put_u8(2);
                        w.put_u32(ratio);
                    }
                }
            }
            Entry::DayHash { state, log } => {
                w.put_u8(4);
                w.put_u64(state);
                w.put_u64(log);
            }
        }
        w.into_bytes()
    }

    /// # Errors
    /// Any `DecodeError`: wrong version, unknown tag, too short or too long.
    pub fn decode(bytes: &[u8]) -> Result<Self, DecodeError> {
        let mut r = Reader::new(bytes);
        let version = r.u16()?;
        if version != RECORD_FORMAT {
            return Err(DecodeError::UnsupportedVersion(version));
        }
        let at = GameTime(r.i64()?);
        let entry = match r.u8()? {
            0 => Entry::Ruleset(r.u32()?),
            1 => Entry::Seed(r.u64()?),
            2 => Entry::Order(Order::decode(&mut r)?),
            3 => Entry::Admin(match r.u8()? {
                0 => AdminAction::Pause,
                1 => AdminAction::Resume,
                2 => AdminAction::SetRatio(r.u32()?),
                tag => return Err(DecodeError::BadTag(tag)),
            }),
            4 => Entry::DayHash {
                state: r.u64()?,
                log: r.u64()?,
            },
            tag => return Err(DecodeError::BadTag(tag)),
        };
        r.finish()?;
        Ok(Self { at, entry })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReplayError {
    Snapshot(DecodeError),
    /// Written under another ruleset version; replay works within one (TM9). `index` is the
    /// log record, or `None` for the snapshot.
    Ruleset {
        index: Option<usize>,
        found: u32,
    },
    /// The snapshot covers more records than the log holds.
    LogTooShort,
    /// The record at this index is before the world's clock, or a daily hash is missing or
    /// misplaced.
    OutOfOrder(usize),
    /// The record at this index failed to apply, e.g. an order the live world accepted.
    Sim(usize, SimError),
    /// The daily hash at this index differs from the replayed one: state or log diverged.
    Diverged(usize),
}

impl ReplayError {
    /// The log record where replay stopped, if it got that far.
    #[must_use]
    pub const fn record_index(&self) -> Option<usize> {
        match *self {
            Self::Snapshot(_) | Self::LogTooShort => None,
            Self::Ruleset { index, .. } => index,
            Self::OutOfOrder(i) | Self::Sim(i, _) | Self::Diverged(i) => Some(i),
        }
    }
}

impl fmt::Display for ReplayError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Snapshot(e) => write!(f, "bad snapshot: {e}"),
            Self::Ruleset { found, .. } => write!(
                f,
                "written by ruleset {found}, this build runs {RULESET_VERSION}"
            ),
            Self::LogTooShort => f.write_str("log is shorter than the snapshot claims"),
            Self::OutOfOrder(i) => write!(f, "record {i} is out of order"),
            Self::Sim(i, e) => write!(f, "record {i} failed: {e}"),
            Self::Diverged(i) => write!(f, "daily hash in record {i} does not match"),
        }
    }
}

impl std::error::Error for ReplayError {}

impl From<DecodeError> for ReplayError {
    fn from(e: DecodeError) -> Self {
        Self::Snapshot(e)
    }
}

/// A world plus its input log: every input goes through here, so the log always matches.
#[derive(Debug)]
pub struct Recorder {
    world: World,
    log: Vec<Record>,
    /// Digest of the encoded records since the last `DayHash`.
    digest: StateHasher,
    /// The next game-day boundary, where a `DayHash` is due.
    next_hash: GameTime,
}

impl Recorder {
    /// A new world (see `World::new`); the log opens with the ruleset version and the seed.
    ///
    /// # Errors
    /// As `World::new`.
    pub fn create(seed: u64, scenario: &Scenario) -> Result<Self, SimError> {
        let world = World::new(seed, scenario)?;
        let next_hash = next_boundary(world.now())?;
        let mut rec = Self {
            world,
            log: Vec::new(),
            digest: StateHasher::new(),
            next_hash,
        };
        rec.append(Entry::Ruleset(RULESET_VERSION))?;
        rec.append(Entry::Seed(seed))?;
        Ok(rec)
    }

    /// Runs the world to `t`, logging a `DayHash` at every day boundary on the way.
    ///
    /// # Errors
    /// As `World::run_until`.
    pub fn advance_to(&mut self, t: GameTime) -> Result<(), SimError> {
        if t < self.world.now() {
            return Err(SimError::TimeBeforeNow);
        }
        while self.next_hash <= t {
            self.world.run_until(self.next_hash)?;
            let entry = self.day_hash();
            self.append(entry)?;
        }
        self.world.run_until(t)
    }

    /// Validates and applies `order` at the current second, then logs it. A rejected order is
    /// not logged.
    ///
    /// # Errors
    /// As `World::apply_order`.
    pub fn submit(&mut self, order: Order) -> Result<(), SimError> {
        self.world.apply_order(order)?;
        self.append(Entry::Order(order))
    }

    /// # Errors
    /// None in practice; shares `append`'s signature.
    pub fn admin(&mut self, action: AdminAction) -> Result<(), SimError> {
        self.append(Entry::Admin(action))
    }

    /// The full state, enough to resume from with the log records after it (TS6). Header:
    /// format version, ruleset version, number of log records it covers, the running digest.
    #[must_use]
    pub fn snapshot(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.put_u16(SNAPSHOT_FORMAT);
        w.put_u32(RULESET_VERSION);
        w.put_len(self.log.len());
        w.put_u64(self.digest.finish());
        self.world.write_state(&mut w);
        w.into_bytes()
    }

    /// Loads `snapshot` and re-runs every record of `log` after the ones it covers. The result
    /// equals the live recorder at the end of `log`, and can carry on live from there.
    ///
    /// # Errors
    /// A bad snapshot, a record from another ruleset or out of order, an order that fails, or
    /// a daily hash that does not match.
    pub fn replay(snapshot: &[u8], log: &[Record]) -> Result<Self, ReplayError> {
        let mut r = Reader::new(snapshot);
        let version = r.u16()?;
        if !(1..=SNAPSHOT_FORMAT).contains(&version) {
            return Err(DecodeError::UnsupportedVersion(version).into());
        }
        let ruleset = r.u32()?;
        if ruleset != RULESET_VERSION {
            return Err(ReplayError::Ruleset {
                index: None,
                found: ruleset,
            });
        }
        let covered = r.read_len()?;
        let digest = StateHasher::resume(r.u64()?);
        let world = World::decode(&mut r, version)?;
        r.finish()?;

        let next_hash = next_boundary(world.now()).map_err(|_| DecodeError::Invalid("time"))?;
        let mut rec = Self {
            world,
            log: log.get(..covered).ok_or(ReplayError::LogTooShort)?.to_vec(),
            digest,
            next_hash,
        };
        for (i, &record) in log.iter().enumerate().skip(covered) {
            rec.replay_record(i, record)?;
        }
        Ok(rec)
    }

    fn replay_record(&mut self, i: usize, record: Record) -> Result<(), ReplayError> {
        let in_order = record.at >= self.world.now()
            && match record.entry {
                Entry::DayHash { .. } => record.at == self.next_hash,
                _ => record.at < self.next_hash,
            };
        if !in_order {
            return Err(ReplayError::OutOfOrder(i));
        }
        let sim = |e| ReplayError::Sim(i, e);
        self.world.run_until(record.at).map_err(sim)?;
        match record.entry {
            Entry::Ruleset(found) if found != RULESET_VERSION => {
                return Err(ReplayError::Ruleset {
                    index: Some(i),
                    found,
                });
            }
            Entry::Order(order) => self.world.apply_order(order).map_err(sim)?,
            Entry::DayHash { .. } if record.entry != self.day_hash() => {
                return Err(ReplayError::Diverged(i));
            }
            Entry::Ruleset(_) | Entry::Seed(_) | Entry::Admin(_) | Entry::DayHash { .. } => {}
        }
        self.append(record.entry).map_err(sim)
    }

    fn day_hash(&self) -> Entry {
        Entry::DayHash {
            state: self.world.state_hash(),
            log: self.digest.finish(),
        }
    }

    /// Logs `entry` at the current second and keeps the digest and the next hash time in step.
    fn append(&mut self, entry: Entry) -> Result<(), SimError> {
        let record = Record {
            at: self.world.now(),
            entry,
        };
        if let Entry::DayHash { .. } = entry {
            self.digest = StateHasher::new();
            self.next_hash = next_boundary(record.at)?;
        } else {
            self.digest.write_bytes(&record.encode());
        }
        self.log.push(record);
        Ok(())
    }

    #[must_use]
    pub const fn world(&self) -> &World {
        &self.world
    }

    #[must_use]
    pub fn log(&self) -> &[Record] {
        &self.log
    }
}

/// The first day boundary strictly after `t`.
fn next_boundary(t: GameTime) -> Result<GameTime, SimError> {
    t.day()
        .checked_add(1)
        .and_then(|d| d.checked_mul(SECONDS_PER_DAY))
        .map(GameTime)
        .ok_or(SimError::TimeOverflow)
}
