//! The world: flat state tables indexed by typed IDs (TS5), the event queue and the run loop.

use std::fmt;

use crate::codec::{DecodeError, Reader, Writer};
use crate::economy;
use crate::event::Event;
use crate::fixed::{Fixed, FixedError};
use crate::hash::StateHasher;
use crate::id::{NationId, ProvinceId};
use crate::order::Order;
use crate::queue::{EventKey, EventQueue, Priority};
use crate::rng::{Rng, streams};
use crate::time::{GameTime, SECONDS_PER_DAY};

/// Bumped whenever the hash encoding below changes, so old and new hashes never match by
/// accident.
const HASH_VERSION: u32 = 1;

const DAY: u64 = SECONDS_PER_DAY.unsigned_abs();

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Province {
    pub owner: NationId,
    /// Income this province adds to its owner per game-day (economy stub).
    pub base_income: Fixed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Nation {
    /// Treasury as of `settled_at`; the live balance adds `income_per_day` for the time since.
    pub treasury: Fixed,
    pub income_per_day: Fixed,
    pub settled_at: GameTime,
    /// Seconds into each game-day at which this nation's daily pass runs; drawn once (TM2).
    pub pass_offset: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SimError {
    Fixed(FixedError),
    TimeOverflow,
    /// `run_until` was asked to go back in time.
    TimeBeforeNow,
    UnknownNation(NationId),
    UnknownProvince(ProvinceId),
    /// The order would change nothing (e.g. handing a province to its owner).
    NoChange,
    /// Every second of the day already holds a nation's pass.
    TooManyNations,
}

impl fmt::Display for SimError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Fixed(e) => e.fmt(f),
            Self::TimeOverflow => f.write_str("game time overflow"),
            Self::TimeBeforeNow => f.write_str("target time is before the world's current time"),
            Self::UnknownNation(id) => write!(f, "unknown nation {}", id.0),
            Self::UnknownProvince(id) => write!(f, "unknown province {}", id.0),
            Self::NoChange => f.write_str("the order changes nothing"),
            Self::TooManyNations => f.write_str("no free daily-pass offset left"),
        }
    }
}

impl std::error::Error for SimError {}

impl From<FixedError> for SimError {
    fn from(e: FixedError) -> Self {
        Self::Fixed(e)
    }
}

#[derive(Debug)]
pub struct World {
    now: GameTime,
    nations: Vec<Nation>,
    provinces: Vec<Province>,
    queue: EventQueue<Event>,
    schedule_rng: Rng,
}

impl World {
    /// A world at game-second 0 with `nation_count` nations (IDs `0..nation_count`) and the
    /// given provinces (IDs in slice order).
    ///
    /// # Errors
    /// `UnknownNation` if a province's owner is out of range, `TooManyNations` if there are
    /// more nations than seconds in a day.
    pub fn new(seed: u64, nation_count: u32, provinces: &[Province]) -> Result<Self, SimError> {
        if let Some(p) = provinces.iter().find(|p| p.owner.0 >= nation_count) {
            return Err(SimError::UnknownNation(p.owner));
        }
        let mut world = Self {
            now: GameTime::ZERO,
            nations: Vec::new(),
            provinces: provinces.to_vec(),
            queue: EventQueue::new(),
            schedule_rng: Rng::new(seed, streams::SCHEDULE),
        };
        for i in 0..nation_count {
            world.add_nation(NationId(i))?;
        }
        Ok(world)
    }

    fn add_nation(&mut self, id: NationId) -> Result<(), SimError> {
        let offset = self.draw_pass_offset()?;
        let income_per_day = economy::income_per_day(&self.provinces, id)?;
        self.nations.push(Nation {
            treasury: Fixed::ZERO,
            income_per_day,
            settled_at: self.now,
            pass_offset: offset,
        });
        // Worlds start at second 0, so the first pass is on day 0 at the offset.
        self.schedule(GameTime(offset), Event::DailyPass(id));
        Ok(())
    }

    /// Uniform over the day, redrawn until no other nation has it, so passes never share a
    /// second. The modulo bias (86,400 / 2^64) is far too small to matter.
    fn draw_pass_offset(&mut self) -> Result<i64, SimError> {
        if u64::try_from(self.nations.len()).map_or(true, |n| n >= DAY) {
            return Err(SimError::TooManyNations);
        }
        loop {
            let draw = self.schedule_rng.next_u64() % DAY;
            let offset = i64::try_from(draw).map_err(|_| SimError::TimeOverflow)?;
            if self.nations.iter().all(|n| n.pass_offset != offset) {
                return Ok(offset);
            }
        }
    }

    fn schedule(&mut self, time: GameTime, event: Event) {
        self.queue.push(time, event.priority(), event);
    }

    /// Runs every event up to and including game-second `t`, then sets the clock to `t`.
    ///
    /// # Errors
    /// `TimeBeforeNow` if `t` is in the past; otherwise the first error an event hits. The
    /// world then stops at that event and must not be run further.
    pub fn run_until(&mut self, t: GameTime) -> Result<(), SimError> {
        if t < self.now {
            return Err(SimError::TimeBeforeNow);
        }
        while self.queue.peek_key().is_some_and(|k| k.time <= t) {
            let Some((key, event)) = self.queue.pop() else {
                break;
            };
            self.now = key.time;
            self.apply(event)?;
        }
        self.now = t;
        Ok(())
    }

    fn apply(&mut self, event: Event) -> Result<(), SimError> {
        match event {
            Event::DailyPass(nation) => {
                economy::rerate(&mut self.nations, &self.provinces, nation, self.now)?;
                let next = self
                    .now
                    .checked_add_seconds(SECONDS_PER_DAY)
                    .ok_or(SimError::TimeOverflow)?;
                self.schedule(next, event);
            }
        }
        Ok(())
    }

    /// Validates `order` and applies it at the current second. On a validation error nothing
    /// has changed; any later error leaves the world unusable, as in `run_until`.
    ///
    /// # Errors
    /// `UnknownProvince`, `UnknownNation` or `NoChange` if the order is invalid; otherwise the
    /// first error applying it hits.
    pub fn apply_order(&mut self, order: Order) -> Result<(), SimError> {
        match order {
            Order::TransferProvince { province, to } => {
                let p = self
                    .provinces
                    .get_mut(province.index())
                    .ok_or(SimError::UnknownProvince(province))?;
                if to.index() >= self.nations.len() {
                    return Err(SimError::UnknownNation(to));
                }
                if p.owner == to {
                    return Err(SimError::NoChange);
                }
                let from = std::mem::replace(&mut p.owner, to);
                economy::rerate(&mut self.nations, &self.provinces, from, self.now)?;
                economy::rerate(&mut self.nations, &self.provinces, to, self.now)?;
            }
        }
        Ok(())
    }

    /// Hash of the entire state (TM9): clock, RNG streams, every table in ID order and the
    /// pending queue. Two worlds with equal hashes run identically from here on.
    #[must_use]
    pub fn state_hash(&self) -> u64 {
        let mut h = StateHasher::new();
        h.write_u32(HASH_VERSION);
        h.write_i64(self.now.seconds());
        for word in self.schedule_rng.state_words() {
            h.write_u64(word);
        }
        h.write_len(self.nations.len());
        for n in &self.nations {
            h.write_i64(n.treasury.to_bits());
            h.write_i64(n.income_per_day.to_bits());
            h.write_i64(n.settled_at.seconds());
            h.write_i64(n.pass_offset);
        }
        h.write_len(self.provinces.len());
        for p in &self.provinces {
            h.write_u32(p.owner.0);
            h.write_i64(p.base_income.to_bits());
        }
        h.write_u64(self.queue.next_sequence());
        let pending = self.queue.sorted();
        h.write_len(pending.len());
        for (key, event) in pending {
            h.write_i64(key.time.seconds());
            h.write_u16(key.priority.0);
            h.write_u64(key.sequence);
            event.hash_into(&mut h);
        }
        h.finish()
    }

    /// The snapshot body (TS6): the same state the hash covers, in the same order. The
    /// snapshot header carries the format version; this layout is format version 1.
    pub(crate) fn encode(&self, w: &mut Writer) {
        w.put_i64(self.now.seconds());
        for word in self.schedule_rng.state_words() {
            w.put_u64(word);
        }
        w.put_len(self.nations.len());
        for n in &self.nations {
            w.put_i64(n.treasury.to_bits());
            w.put_i64(n.income_per_day.to_bits());
            w.put_i64(n.settled_at.seconds());
            w.put_i64(n.pass_offset);
        }
        w.put_len(self.provinces.len());
        for p in &self.provinces {
            w.put_u32(p.owner.0);
            w.put_i64(p.base_income.to_bits());
        }
        w.put_u64(self.queue.next_sequence());
        let pending = self.queue.sorted();
        w.put_len(pending.len());
        for (key, event) in pending {
            w.put_i64(key.time.seconds());
            w.put_u16(key.priority.0);
            w.put_u64(key.sequence);
            event.encode(w);
        }
    }

    /// Reads what `encode` wrote. Rejects IDs out of range and events in the past, so a
    /// corrupt snapshot fails here instead of misbehaving later.
    pub(crate) fn decode(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        let now = GameTime(r.i64()?);
        let words = [r.u64()?, r.u64()?, r.u64()?, r.u64()?];
        let schedule_rng = Rng::from_state_words(words).ok_or(DecodeError::Invalid("rng"))?;
        let mut nations = Vec::new();
        for _ in 0..r.read_len()? {
            nations.push(Nation {
                treasury: Fixed::from_bits(r.i64()?),
                income_per_day: Fixed::from_bits(r.i64()?),
                settled_at: GameTime(r.i64()?),
                pass_offset: r.i64()?,
            });
        }
        let known = |id: NationId| id.index() < nations.len();
        let mut provinces = Vec::new();
        for _ in 0..r.read_len()? {
            let owner = NationId(r.u32()?);
            if !known(owner) {
                return Err(DecodeError::Invalid("province owner"));
            }
            provinces.push(Province {
                owner,
                base_income: Fixed::from_bits(r.i64()?),
            });
        }
        let next_sequence = r.u64()?;
        let mut pending = Vec::new();
        for _ in 0..r.read_len()? {
            let key = EventKey {
                time: GameTime(r.i64()?),
                priority: Priority(r.u16()?),
                sequence: r.u64()?,
            };
            let event = Event::decode(r)?;
            let Event::DailyPass(nation) = event;
            if key.time < now || !known(nation) {
                return Err(DecodeError::Invalid("pending event"));
            }
            pending.push((key, event));
        }
        let queue =
            EventQueue::from_parts(next_sequence, pending).ok_or(DecodeError::Invalid("queue"))?;
        Ok(Self {
            now,
            nations,
            provinces,
            queue,
            schedule_rng,
        })
    }

    #[must_use]
    pub const fn now(&self) -> GameTime {
        self.now
    }

    #[must_use]
    pub fn nations(&self) -> &[Nation] {
        &self.nations
    }

    #[must_use]
    pub fn provinces(&self) -> &[Province] {
        &self.provinces
    }
}
