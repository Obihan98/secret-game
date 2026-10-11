//! The world: flat state tables indexed by typed IDs (TS5), the event queue and the run loop.

use std::fmt;

use crate::codec::{DecodeError, Reader, Sink};
use crate::economy;
use crate::event::Event;
use crate::fixed::{Fixed, FixedError};
use crate::hash::StateHasher;
use crate::id::{ArmyId, NationId, ProvinceId};
use crate::order::Order;
use crate::queue::{EventKey, EventQueue, Priority};
use crate::rng::{Rng, streams};
use crate::time::{GameTime, SECONDS_PER_DAY};

/// Bumped whenever the hashed bytes change (they are the snapshot body, `write_state`), so old
/// and new hashes never match by accident.
const HASH_VERSION: u32 = 2;

const DAY: u64 = SECONDS_PER_DAY.unsigned_abs();

/// MP3 (minimal): for now terrain only sets march speed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Terrain {
    Plains,
    Forest,
    Hills,
    Mountains,
}

impl Terrain {
    /// Part of the snapshot format: never reuse or renumber a tag.
    const fn tag(self) -> u8 {
        match self {
            Self::Plains => 0,
            Self::Forest => 1,
            Self::Hills => 2,
            Self::Mountains => 3,
        }
    }

    const fn from_tag(tag: u8) -> Option<Self> {
        match tag {
            0 => Some(Self::Plains),
            1 => Some(Self::Forest),
            2 => Some(Self::Hills),
            3 => Some(Self::Mountains),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Province {
    pub owner: NationId,
    /// Income this province adds to its owner per game-day (economy stub).
    pub base_income: Fixed,
    pub terrain: Terrain,
}

/// A road between two adjacent provinces, usable both ways (MP7).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Border {
    pub a: ProvinceId,
    pub b: ProvinceId,
    pub length_m: u32,
}

/// One road being marched, or halted on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Leg {
    pub to: ProvinceId,
    pub length_m: u32,
    /// Meters covered as of `settled_at`; the live position adds `speed` for the time since.
    pub done_m: u32,
    pub settled_at: GameTime,
    /// Meters per game-day; 0 = halted on the road after a contact.
    pub speed: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Position {
    /// Standing in a province.
    In(ProvinceId),
    /// On the road from `from` to `leg.to`.
    Road { from: ProvinceId, leg: Leg },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Army {
    pub owner: NationId,
    pub position: Position,
    /// Provinces still to enter after the current road, the next one last (popped off the end).
    pub route: Vec<ProvinceId>,
    /// Bumped whenever the army's position or speed changes. Arrive and Meet events carry the
    /// value they were planned with and do nothing once it has moved on.
    pub generation: u32,
}

/// Two armies of different nations met (contact stub: no combat until CB is designed).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Contact {
    pub at: GameTime,
    pub a: ArmyId,
    pub b: ArmyId,
}

/// Everything a new world starts with. Province, border and army IDs follow vector order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Scenario {
    pub nations: u32,
    pub provinces: Vec<Province>,
    pub borders: Vec<Border>,
    /// Starting armies: owner and the province each stands in.
    pub armies: Vec<(NationId, ProvinceId)>,
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
    UnknownArmy(ArmyId),
    /// The scenario's border at this index joins a province to itself, names one that does
    /// not exist, has length 0, or repeats another border.
    BadBorder(usize),
    /// No road leads to the march destination.
    Unreachable,
    TooManyArmies,
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
            Self::UnknownArmy(id) => write!(f, "unknown army {}", id.0),
            Self::BadBorder(i) => write!(f, "border {i} is invalid"),
            Self::Unreachable => f.write_str("no road leads there"),
            Self::TooManyArmies => f.write_str("too many armies"),
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

#[derive(Clone, Debug)]
pub struct World {
    pub(crate) now: GameTime,
    pub(crate) nations: Vec<Nation>,
    pub(crate) provinces: Vec<Province>,
    pub(crate) borders: Vec<Border>,
    /// Derived from `borders`, not saved: each province's neighbours and road lengths, sorted
    /// by neighbour ID so route search is deterministic.
    pub(crate) neighbours: Vec<Vec<(ProvinceId, u32)>>,
    pub(crate) armies: Vec<Army>,
    pub(crate) contacts: Vec<Contact>,
    queue: EventQueue<Event>,
    schedule_rng: Rng,
}

impl World {
    /// A world at game-second 0 set up as `scenario` describes.
    ///
    /// # Errors
    /// `UnknownNation` or `UnknownProvince` for an owner or army location out of range,
    /// `BadBorder` for an invalid border, `TooManyNations` if there are more nations than
    /// seconds in a day.
    pub fn new(seed: u64, scenario: &Scenario) -> Result<Self, SimError> {
        let Scenario {
            nations,
            provinces,
            borders,
            armies,
        } = scenario;
        if let Some(p) = provinces.iter().find(|p| p.owner.0 >= *nations) {
            return Err(SimError::UnknownNation(p.owner));
        }
        let neighbours = build_neighbours(provinces.len(), borders).map_err(SimError::BadBorder)?;
        for &(owner, at) in armies {
            if owner.0 >= *nations {
                return Err(SimError::UnknownNation(owner));
            }
            if at.index() >= provinces.len() {
                return Err(SimError::UnknownProvince(at));
            }
        }
        if u32::try_from(armies.len()).is_err() {
            return Err(SimError::TooManyArmies);
        }
        let mut world = Self {
            now: GameTime::ZERO,
            nations: Vec::new(),
            provinces: provinces.clone(),
            borders: borders.clone(),
            neighbours,
            armies: armies
                .iter()
                .map(|&(owner, at)| Army {
                    owner,
                    position: Position::In(at),
                    route: Vec::new(),
                    generation: 0,
                })
                .collect(),
            contacts: Vec::new(),
            queue: EventQueue::new(),
            schedule_rng: Rng::new(seed, streams::SCHEDULE),
        };
        for i in 0..*nations {
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

    pub(crate) fn schedule(&mut self, time: GameTime, event: Event) {
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
                self.movement_pass(nation)?;
                let next = self
                    .now
                    .checked_add_seconds(SECONDS_PER_DAY)
                    .ok_or(SimError::TimeOverflow)?;
                self.schedule(next, event);
            }
            Event::Arrive { army, generation } => self.arrive(army, generation)?,
            Event::Meet {
                a,
                b,
                generation_a,
                generation_b,
            } => self.meet(a, b, generation_a, generation_b)?,
        }
        Ok(())
    }

    /// Validates `order` and applies it at the current second. On a validation error nothing
    /// has changed; any later error leaves the world unusable, as in `run_until`.
    ///
    /// # Errors
    /// `UnknownProvince`, `UnknownNation`, `UnknownArmy`, `Unreachable` or `NoChange` if the
    /// order is invalid; otherwise the first error applying it hits.
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
            Order::March { army, to } => self.order_march(army, to)?,
        }
        Ok(())
    }

    /// Hash of the entire state (TM9): exactly the snapshot body, so a hash match means the
    /// saved worlds are byte-identical and run identically from here on.
    #[must_use]
    pub fn state_hash(&self) -> u64 {
        let mut h = StateHasher::new();
        h.put_u32(HASH_VERSION);
        self.write_state(&mut h);
        h.finish()
    }

    /// The snapshot body (TS6), in snapshot format version 2: clock, RNG streams, every table
    /// in ID order and the pending queue. Derived data (`neighbours`) is rebuilt on load.
    pub(crate) fn write_state(&self, s: &mut impl Sink) {
        s.put_i64(self.now.seconds());
        for word in self.schedule_rng.state_words() {
            s.put_u64(word);
        }
        s.put_len(self.nations.len());
        for n in &self.nations {
            s.put_i64(n.treasury.to_bits());
            s.put_i64(n.income_per_day.to_bits());
            s.put_i64(n.settled_at.seconds());
            s.put_i64(n.pass_offset);
        }
        s.put_len(self.provinces.len());
        for p in &self.provinces {
            s.put_u32(p.owner.0);
            s.put_i64(p.base_income.to_bits());
            s.put_u8(p.terrain.tag());
        }
        s.put_len(self.borders.len());
        for b in &self.borders {
            s.put_u32(b.a.0);
            s.put_u32(b.b.0);
            s.put_u32(b.length_m);
        }
        s.put_len(self.armies.len());
        for a in &self.armies {
            s.put_u32(a.owner.0);
            match a.position {
                Position::In(p) => {
                    s.put_u8(0);
                    s.put_u32(p.0);
                }
                Position::Road { from, leg } => {
                    s.put_u8(1);
                    s.put_u32(from.0);
                    s.put_u32(leg.to.0);
                    s.put_u32(leg.length_m);
                    s.put_u32(leg.done_m);
                    s.put_i64(leg.settled_at.seconds());
                    s.put_u32(leg.speed);
                }
            }
            s.put_len(a.route.len());
            for p in &a.route {
                s.put_u32(p.0);
            }
            s.put_u32(a.generation);
        }
        s.put_len(self.contacts.len());
        for c in &self.contacts {
            s.put_i64(c.at.seconds());
            s.put_u32(c.a.0);
            s.put_u32(c.b.0);
        }
        s.put_u64(self.queue.next_sequence());
        let pending = self.queue.sorted();
        s.put_len(pending.len());
        for (key, event) in pending {
            s.put_i64(key.time.seconds());
            s.put_u16(key.priority.0);
            s.put_u64(key.sequence);
            event.write(s);
        }
    }

    /// Reads a snapshot body of format `version`. Version 1 (build-order stage 3) had no
    /// terrain, borders, armies or contacts: its provinces load as plains, the rest empty.
    /// Rejects IDs out of range and events in the past, so a corrupt snapshot fails here
    /// instead of misbehaving later.
    pub(crate) fn decode(r: &mut Reader<'_>, version: u16) -> Result<Self, DecodeError> {
        let invalid = DecodeError::Invalid;
        let now = GameTime(r.i64()?);
        let words = [r.u64()?, r.u64()?, r.u64()?, r.u64()?];
        let schedule_rng = Rng::from_state_words(words).ok_or(invalid("rng"))?;
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
                return Err(invalid("province owner"));
            }
            let base_income = Fixed::from_bits(r.i64()?);
            let terrain = if version >= 2 {
                Terrain::from_tag(r.u8()?).ok_or(invalid("terrain"))?
            } else {
                Terrain::Plains
            };
            provinces.push(Province {
                owner,
                base_income,
                terrain,
            });
        }
        let (borders, armies, contacts) = if version >= 2 {
            decode_movement_tables(r)?
        } else {
            (Vec::new(), Vec::new(), Vec::new())
        };
        let neighbours =
            build_neighbours(provinces.len(), &borders).map_err(|_| invalid("border"))?;
        let on_road = |from: ProvinceId, leg: &Leg| {
            leg.done_m <= leg.length_m
                && neighbours
                    .get(from.index())
                    .is_some_and(|n| n.contains(&(leg.to, leg.length_m)))
        };
        let army_known = |id: ArmyId| id.index() < armies.len();
        for a in &armies {
            let at_ok = match &a.position {
                Position::In(p) => p.index() < provinces.len(),
                Position::Road { from, leg } => on_road(*from, leg),
            };
            let route_ok = a.route.iter().all(|p| p.index() < provinces.len());
            if !known(a.owner) || !at_ok || !route_ok {
                return Err(invalid("army"));
            }
        }
        if !contacts.iter().all(|c| army_known(c.a) && army_known(c.b)) {
            return Err(invalid("contact"));
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
            let refs_ok = match event {
                Event::DailyPass(nation) => known(nation),
                Event::Arrive { army, .. } => army_known(army),
                Event::Meet { a, b, .. } => army_known(a) && army_known(b),
            };
            if key.time < now || !refs_ok {
                return Err(invalid("pending event"));
            }
            pending.push((key, event));
        }
        let queue = EventQueue::from_parts(next_sequence, pending).ok_or(invalid("queue"))?;
        Ok(Self {
            now,
            nations,
            provinces,
            borders,
            neighbours,
            armies,
            contacts,
            queue,
            schedule_rng,
        })
    }

    /// Total events ever scheduled: lets tests count reschedules.
    #[must_use]
    pub const fn events_scheduled(&self) -> u64 {
        self.queue.next_sequence()
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

    #[must_use]
    pub fn borders(&self) -> &[Border] {
        &self.borders
    }

    #[must_use]
    pub fn armies(&self) -> &[Army] {
        &self.armies
    }

    #[must_use]
    pub fn contacts(&self) -> &[Contact] {
        &self.contacts
    }
}

/// The tables snapshot format 2 added (build-order stage 6), unchecked: `decode` validates.
type MovementTables = (Vec<Border>, Vec<Army>, Vec<Contact>);

fn decode_movement_tables(r: &mut Reader<'_>) -> Result<MovementTables, DecodeError> {
    let (mut borders, mut armies, mut contacts) = (Vec::new(), Vec::new(), Vec::new());
    for _ in 0..r.read_len()? {
        borders.push(Border {
            a: ProvinceId(r.u32()?),
            b: ProvinceId(r.u32()?),
            length_m: r.u32()?,
        });
    }
    for _ in 0..r.read_len()? {
        armies.push(decode_army(r)?);
    }
    for _ in 0..r.read_len()? {
        contacts.push(Contact {
            at: GameTime(r.i64()?),
            a: ArmyId(r.u32()?),
            b: ArmyId(r.u32()?),
        });
    }
    Ok((borders, armies, contacts))
}

fn decode_army(r: &mut Reader<'_>) -> Result<Army, DecodeError> {
    let owner = NationId(r.u32()?);
    let position = match r.u8()? {
        0 => Position::In(ProvinceId(r.u32()?)),
        1 => Position::Road {
            from: ProvinceId(r.u32()?),
            leg: Leg {
                to: ProvinceId(r.u32()?),
                length_m: r.u32()?,
                done_m: r.u32()?,
                settled_at: GameTime(r.i64()?),
                speed: r.u32()?,
            },
        },
        tag => return Err(DecodeError::BadTag(tag)),
    };
    let mut route = Vec::new();
    for _ in 0..r.read_len()? {
        route.push(ProvinceId(r.u32()?));
    }
    Ok(Army {
        owner,
        position,
        route,
        generation: r.u32()?,
    })
}

/// Each province's neighbours and road lengths, sorted by neighbour ID. `Err(i)` names the
/// first invalid border.
fn build_neighbours(
    province_count: usize,
    borders: &[Border],
) -> Result<Vec<Vec<(ProvinceId, u32)>>, usize> {
    let mut neighbours = vec![Vec::new(); province_count];
    for (i, b) in borders.iter().enumerate() {
        let valid = b.a != b.b
            && b.length_m > 0
            && b.a.index() < province_count
            && b.b.index() < province_count
            && !neighbours[b.a.index()].iter().any(|&(q, _)| q == b.b);
        if !valid {
            return Err(i);
        }
        neighbours[b.a.index()].push((b.b, b.length_m));
        neighbours[b.b.index()].push((b.a, b.length_m));
    }
    for n in &mut neighbours {
        n.sort_unstable();
    }
    Ok(neighbours)
}
