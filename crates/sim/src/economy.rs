//! Economy stub (build-order stage 2): just enough to exercise the daily pass. The real
//! economy is designed at stage 10.
//!
//! A balance is `treasury + income_per_day * elapsed` (TM2). The daily pass and every sudden
//! change (a province changing hands) settle that into the treasury and recompute the rate.

use crate::fixed::Fixed;
use crate::id::NationId;
use crate::time::{GameTime, SECONDS_PER_DAY};
use crate::world::{Nation, Province, SimError};

/// Settles `nation`'s balance at `now` and recomputes its rate from the provinces it owns.
pub(crate) fn rerate(
    nations: &mut [Nation],
    provinces: &[Province],
    nation: NationId,
    now: GameTime,
) -> Result<(), SimError> {
    let income = income_per_day(provinces, nation)?;
    let n = nations
        .get_mut(nation.index())
        .ok_or(SimError::UnknownNation(nation))?;
    settle(n, now)?;
    n.income_per_day = income;
    Ok(())
}

/// Folds the income earned since the last settlement into the treasury.
fn settle(n: &mut Nation, now: GameTime) -> Result<(), SimError> {
    let elapsed = now.seconds() - n.settled_at.seconds();
    let earned = n.income_per_day.mul_ratio(elapsed, SECONDS_PER_DAY)?;
    n.treasury = n.treasury.checked_add(earned)?;
    n.settled_at = now;
    Ok(())
}

pub(crate) fn income_per_day(provinces: &[Province], nation: NationId) -> Result<Fixed, SimError> {
    provinces
        .iter()
        .filter(|p| p.owner == nation)
        .try_fold(Fixed::ZERO, |sum, p| Ok(sum.checked_add(p.base_income)?))
}
