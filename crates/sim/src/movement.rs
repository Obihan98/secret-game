//! Movement (MP7, MP3; build-order stage 6): armies march province to adjacent province along
//! borders. Arrival is an exact-timestamp event; a nation's daily pass recomputes its armies'
//! speeds and reschedules only the arrivals whose speed changed (TM2, T12). Meetings between
//! armies are found whenever a road is planned or re-planned.
//!
//! Stubs until their systems are designed (agreed 2026-10-11): winter (TM4, MP6) and being out
//! of supply (AR8, here: on foreign soil) each slow an army by 25%; armies of different nations
//! that meet both halt and a contact is recorded, with no combat (CB).

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use crate::event::Event;
use crate::id::{ArmyId, NationId, ProvinceId};
use crate::time::{GameTime, SECONDS_PER_DAY};
use crate::world::{Army, Contact, Leg, Position, Province, SimError, Terrain, World};

/// Meters per game-day on plains, in supply, outside winter: the historical 20 km/day (MP7).
pub const BASE_SPEED: u32 = 20_000;

impl Terrain {
    /// Speed in quarters of the plains speed (MP3).
    const fn speed_quarters(self) -> u32 {
        match self {
            Self::Plains => 4,
            Self::Forest | Self::Hills => 3,
            Self::Mountains => 2,
        }
    }
}

/// Winter stub until the calendar (TM4) and weather (MP6) are designed: December to February,
/// on 365-day years counted from 1 January 1800 (WD3), no leap days.
#[must_use]
pub const fn is_winter(t: GameTime) -> bool {
    let day_of_year = t.day().rem_euclid(365);
    day_of_year < 59 || day_of_year >= 334
}

/// Meters covered on `leg` by `t`, rounded down.
fn progress(leg: &Leg, t: GameTime) -> u32 {
    let elapsed = i128::from(t.seconds() - leg.settled_at.seconds()).max(0);
    let moved = i128::from(leg.speed) * elapsed / i128::from(SECONDS_PER_DAY);
    let done = (i128::from(leg.done_m) + moved).min(i128::from(leg.length_m));
    u32::try_from(done).unwrap_or(leg.length_m)
}

/// `n / d` rounded up, for `n >= 0`, `d > 0`.
const fn div_ceil(n: i128, d: i128) -> i128 {
    (n + d - 1) / d
}

/// When a marching leg ends, rounded up so an army never arrives before it has covered the
/// road. `None` while halted.
fn arrival(leg: &Leg) -> Result<Option<GameTime>, SimError> {
    if leg.speed == 0 {
        return Ok(None);
    }
    let rest = i128::from(leg.length_m.saturating_sub(leg.done_m));
    let seconds = div_ceil(rest * i128::from(SECONDS_PER_DAY), i128::from(leg.speed));
    i64::try_from(seconds)
        .ok()
        .and_then(|s| leg.settled_at.checked_add_seconds(s))
        .map(Some)
        .ok_or(SimError::TimeOverflow)
}

/// When armies on legs `a` and `b` of the same road first share a point, as seen at `now`.
/// `reverse` means they head opposite ways. Positions are measured along `a`'s direction; the
/// gap closes only if the one behind is faster, or if they head toward each other.
fn contact_time(a: &Leg, b: &Leg, reverse: bool, now: GameTime) -> Option<GameTime> {
    let length = i128::from(a.length_m);
    let xa = i128::from(progress(a, now));
    let (xb, vb) = if reverse {
        (length - i128::from(progress(b, now)), -i128::from(b.speed))
    } else {
        (i128::from(progress(b, now)), i128::from(b.speed))
    };
    let gap = xb - xa;
    let closing = i128::from(a.speed) - vb;
    if gap == 0 {
        // Side by side: meeting now if crossing, not if they set out together.
        return (reverse && closing != 0).then_some(now);
    }
    if closing == 0 || gap.signum() != closing.signum() {
        return None;
    }
    let seconds = div_ceil(gap.abs() * i128::from(SECONDS_PER_DAY), closing.abs());
    let t = now.checked_add_seconds(i64::try_from(seconds).ok()?)?;
    // Heading the same way, the one ahead may leave the road before it is caught.
    if !reverse {
        let leader = if gap > 0 { b } else { a };
        if arrival(leader).ok()?.is_some_and(|end| t > end) {
            return None;
        }
    }
    Some(t)
}

/// Route cost of one road, proportional to the time it takes at full speed into `terrain`.
fn road_cost(length_m: u32, terrain: Terrain) -> u64 {
    u64::from(length_m) * 12 / u64::from(terrain.speed_quarters())
}

fn army_id(index: usize) -> Result<ArmyId, SimError> {
    u32::try_from(index)
        .map(ArmyId)
        .map_err(|_| SimError::TooManyArmies)
}

impl World {
    fn army(&self, id: ArmyId) -> Result<&Army, SimError> {
        self.armies.get(id.index()).ok_or(SimError::UnknownArmy(id))
    }

    fn army_mut(&mut self, id: ArmyId) -> Result<&mut Army, SimError> {
        self.armies
            .get_mut(id.index())
            .ok_or(SimError::UnknownArmy(id))
    }

    fn province(&self, id: ProvinceId) -> Result<&Province, SimError> {
        self.provinces
            .get(id.index())
            .ok_or(SimError::UnknownProvince(id))
    }

    fn road_length(&self, from: ProvinceId, to: ProvinceId) -> Option<u32> {
        self.neighbours
            .get(from.index())?
            .iter()
            .find(|&&(q, _)| q == to)
            .map(|&(_, length)| length)
    }

    /// Speed of an army of `owner` on the road into `to`, in meters per game-day. Terrain of
    /// the province entered, then winter, then supply; each step rounds down.
    fn march_speed(&self, owner: NationId, to: ProvinceId) -> Result<u32, SimError> {
        let p = self.province(to)?;
        let mut speed = BASE_SPEED * p.terrain.speed_quarters() / 4;
        if is_winter(self.now) {
            speed = speed * 3 / 4;
        }
        if p.owner != owner {
            speed = speed * 3 / 4;
        }
        Ok(speed)
    }

    /// Fastest path from any of `sources` (province, cost already spent to reach it) to
    /// `dest`, as provinces starting with the source used. Dijkstra; ties go to the lower
    /// province ID, so the choice is deterministic.
    fn route(&self, sources: &[(ProvinceId, u64)], dest: ProvinceId) -> Option<Vec<ProvinceId>> {
        let n = self.provinces.len();
        let mut best = vec![u64::MAX; n];
        let mut prev: Vec<Option<ProvinceId>> = vec![None; n];
        let mut open = BinaryHeap::new();
        for &(p, cost) in sources {
            if cost < *best.get(p.index())? {
                best[p.index()] = cost;
                open.push(Reverse((cost, p)));
            }
        }
        while let Some(Reverse((cost, p))) = open.pop() {
            if p == dest {
                break;
            }
            if cost > best[p.index()] {
                continue;
            }
            for &(q, length) in &self.neighbours[p.index()] {
                let next = cost + road_cost(length, self.provinces[q.index()].terrain);
                if next < best[q.index()] {
                    best[q.index()] = next;
                    prev[q.index()] = Some(p);
                    open.push(Reverse((next, q)));
                }
            }
        }
        if *best.get(dest.index())? == u64::MAX {
            return None;
        }
        let mut path = vec![dest];
        while let Some(p) = prev[path[path.len() - 1].index()] {
            path.push(p);
        }
        path.reverse();
        Some(path)
    }

    /// The `March` order (MP7, MP8 stub: the game picks the fastest route).
    pub(crate) fn order_march(&mut self, id: ArmyId, to: ProvinceId) -> Result<(), SimError> {
        self.province(to)?;
        let army = self.army(id)?;
        let owner = army.owner;
        match army.position {
            Position::In(here) => {
                if here == to {
                    return Err(SimError::NoChange);
                }
                let path = self.route(&[(here, 0)], to).ok_or(SimError::Unreachable)?;
                self.army_mut(id)?.route = path.into_iter().skip(1).rev().collect();
                self.start_leg(id)
            }
            // Marching: finish the road it is on, then follow the new route.
            Position::Road { leg, .. } if leg.speed > 0 => {
                let path = self
                    .route(&[(leg.to, 0)], to)
                    .ok_or(SimError::Unreachable)?;
                let route: Vec<_> = path.into_iter().skip(1).rev().collect();
                let army = self.army_mut(id)?;
                if route == army.route {
                    return Err(SimError::NoChange);
                }
                army.route = route;
                Ok(())
            }
            // Halted on a road: go on or turn back, whichever is faster.
            Position::Road { from, leg } => {
                let ahead = road_cost(leg.length_m - leg.done_m, self.province(leg.to)?.terrain);
                let back = road_cost(leg.done_m, self.province(from)?.terrain);
                let path = self
                    .route(&[(leg.to, ahead), (from, back)], to)
                    .ok_or(SimError::Unreachable)?;
                let (from, leg) = if path[0] == leg.to {
                    (from, leg)
                } else {
                    let back = Leg {
                        to: from,
                        done_m: leg.length_m - leg.done_m,
                        ..leg
                    };
                    (leg.to, back)
                };
                let speed = self.march_speed(owner, leg.to)?;
                let now = self.now;
                let army = self.army_mut(id)?;
                army.position = Position::Road {
                    from,
                    leg: Leg {
                        settled_at: now,
                        speed,
                        ..leg
                    },
                };
                army.route = path.into_iter().skip(1).rev().collect();
                self.replan(id)
            }
        }
    }

    /// Puts a standing army on the road to the next province of its route, if any.
    fn start_leg(&mut self, id: ArmyId) -> Result<(), SimError> {
        let army = self.army(id)?;
        let (Position::In(from), Some(&to)) = (army.position, army.route.last()) else {
            return Ok(());
        };
        let length_m = self.road_length(from, to).ok_or(SimError::Unreachable)?;
        let speed = self.march_speed(army.owner, to)?;
        let now = self.now;
        let army = self.army_mut(id)?;
        army.route.pop();
        army.position = Position::Road {
            from,
            leg: Leg {
                to,
                length_m,
                done_m: 0,
                settled_at: now,
                speed,
            },
        };
        self.replan(id)
    }

    /// After any change to an army's road or speed: invalidates its old events, schedules its
    /// arrival and looks for armies it will meet on this road.
    fn replan(&mut self, id: ArmyId) -> Result<(), SimError> {
        let army = self.army_mut(id)?;
        army.generation = army.generation.wrapping_add(1);
        let generation = army.generation;
        if let Position::Road { leg, .. } = army.position
            && let Some(t) = arrival(&leg)?
        {
            self.schedule(
                t,
                Event::Arrive {
                    army: id,
                    generation,
                },
            );
        }
        self.plan_contacts(id)
    }

    /// Schedules a `Meet` with every army of another nation on the same road that `a` will
    /// reach, or that will reach `a`. Each is checked again when either side re-plans.
    fn plan_contacts(&mut self, a: ArmyId) -> Result<(), SimError> {
        let army = self.army(a)?;
        let Position::Road { from, leg } = army.position else {
            return Ok(());
        };
        let mut meetings = Vec::new();
        for (i, other) in self.armies.iter().enumerate() {
            let Position::Road {
                from: other_from,
                leg: other_leg,
            } = other.position
            else {
                continue;
            };
            let same = other_from == from && other_leg.to == leg.to;
            let reverse = other_from == leg.to && other_leg.to == from;
            if i == a.index() || other.owner == army.owner || !(same || reverse) {
                continue;
            }
            if let Some(t) = contact_time(&leg, &other_leg, reverse, self.now) {
                meetings.push((t, army_id(i)?, other.generation));
            }
        }
        let generation_a = army.generation;
        for (t, b, generation_b) in meetings {
            let event = Event::Meet {
                a,
                b,
                generation_a,
                generation_b,
            };
            self.schedule(t, event);
        }
        Ok(())
    }

    pub(crate) fn arrive(&mut self, id: ArmyId, generation: u32) -> Result<(), SimError> {
        let army = self.army(id)?;
        let Position::Road { leg, .. } = army.position else {
            return Ok(());
        };
        if army.generation != generation {
            return Ok(());
        }
        let here = leg.to;
        let owner = army.owner;
        let standing = self
            .armies
            .iter()
            .position(|o| o.owner != owner && o.position == Position::In(here));
        let now = self.now;
        let army = self.army_mut(id)?;
        army.position = Position::In(here);
        army.generation = army.generation.wrapping_add(1);
        if let Some(other) = standing {
            army.route.clear();
            let contact = Contact {
                at: now,
                a: id,
                b: army_id(other)?,
            };
            self.contacts.push(contact);
            return Ok(());
        }
        self.start_leg(id)
    }

    pub(crate) fn meet(
        &mut self,
        a: ArmyId,
        b: ArmyId,
        generation_a: u32,
        generation_b: u32,
    ) -> Result<(), SimError> {
        if self.army(a)?.generation != generation_a || self.army(b)?.generation != generation_b {
            return Ok(());
        }
        let now = self.now;
        for id in [a, b] {
            let army = self.army_mut(id)?;
            if let Position::Road { leg, .. } = &mut army.position {
                leg.done_m = progress(leg, now);
                leg.settled_at = now;
                leg.speed = 0;
            }
            army.route.clear();
        }
        self.contacts.push(Contact { at: now, a, b });
        // Halted armies block the road: anyone else on it may now walk into them.
        self.replan(a)?;
        self.replan(b)
    }

    /// The movement part of `nation`'s daily pass: gradual changes (season, supply) reach its
    /// marching armies here, and only an army whose speed changed is rescheduled.
    pub(crate) fn movement_pass(&mut self, nation: NationId) -> Result<(), SimError> {
        for i in 0..self.armies.len() {
            let army = &self.armies[i];
            let Position::Road { leg, .. } = army.position else {
                continue;
            };
            if army.owner != nation || leg.speed == 0 {
                continue;
            }
            let speed = self.march_speed(nation, leg.to)?;
            if speed == leg.speed {
                continue;
            }
            let now = self.now;
            if let Position::Road { leg, .. } = &mut self.armies[i].position {
                leg.done_m = progress(leg, now);
                leg.settled_at = now;
                leg.speed = speed;
            }
            self.replan(army_id(i)?)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leg(length_m: u32, done_m: u32, speed: u32) -> Leg {
        Leg {
            to: ProvinceId(1),
            length_m,
            done_m,
            settled_at: GameTime::ZERO,
            speed,
        }
    }

    #[test]
    fn winter_is_december_to_february() {
        let day = |d: i64| GameTime(d * SECONDS_PER_DAY);
        assert!(is_winter(day(0)));
        assert!(is_winter(day(58)));
        assert!(!is_winter(day(59)));
        assert!(!is_winter(day(333)));
        assert!(is_winter(day(334)));
        assert!(is_winter(day(365)));
    }

    #[test]
    fn progress_floors_and_arrival_ceils() {
        let l = leg(10_000, 0, 15_000);
        // 15 km/day covers 10 km in 2/3 of a day: 57,600 s exactly.
        assert_eq!(arrival(&l), Ok(Some(GameTime(57_600))));
        assert_eq!(progress(&l, GameTime(57_599)), 9_999);
        assert_eq!(progress(&l, GameTime(57_600)), 10_000);
        // 7 km at 11,250 m/day: 53,760 s exactly; at 8,437 m/day it rounds up.
        assert_eq!(arrival(&leg(7_000, 0, 11_250)), Ok(Some(GameTime(53_760))));
        assert_eq!(arrival(&leg(7_000, 0, 8_437)), Ok(Some(GameTime(71_685))));
        assert_eq!(arrival(&leg(7_000, 0, 0)), Ok(None));
    }

    #[test]
    fn head_on_armies_meet_where_their_distances_add_up() {
        // 30 km road, 20 and 10 km/day toward each other: 1 day, 20 km from a's end.
        let a = leg(30_000, 0, 20_000);
        let b = leg(30_000, 0, 10_000);
        assert_eq!(
            contact_time(&a, &b, true, GameTime::ZERO),
            Some(GameTime(SECONDS_PER_DAY))
        );
    }

    #[test]
    fn same_way_contact_needs_the_one_behind_to_be_faster_and_in_time() {
        let behind = leg(30_000, 0, 20_000);
        let slow_ahead = leg(30_000, 5_000, 10_000);
        // Closes 10 km/day on a 5 km gap: half a day, before the leader arrives (2.5 days).
        assert_eq!(
            contact_time(&behind, &slow_ahead, false, GameTime::ZERO),
            Some(GameTime(SECONDS_PER_DAY / 2))
        );
        assert_eq!(
            contact_time(&slow_ahead, &behind, false, GameTime::ZERO),
            contact_time(&behind, &slow_ahead, false, GameTime::ZERO)
        );
        // The leader is 1 km from the end: it arrives before it can be caught.
        let near_end = leg(30_000, 29_000, 10_000);
        assert_eq!(
            contact_time(&behind, &near_end, false, GameTime::ZERO),
            None
        );
        // Same speed, or the leader faster: never.
        assert_eq!(
            contact_time(
                &slow_ahead,
                &leg(30_000, 6_000, 10_000),
                false,
                GameTime::ZERO
            ),
            None
        );
        // Setting out together is not a meeting.
        assert_eq!(
            contact_time(&behind, &leg(30_000, 0, 10_000), false, GameTime::ZERO),
            None
        );
    }
}
