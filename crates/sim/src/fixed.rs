//! Q32.32 fixed-point numbers for all state math (TM9): no floats, identical on every platform.
//!
//! Results round to nearest, ties to even, so long chains of daily updates do not drift in one
//! direction. Overflow and division by zero are errors; nothing wraps or saturates silently.

use std::fmt;

const FRAC_BITS: u32 = 32;

/// A signed Q32.32 number stored as raw `i64` bits (`value = bits / 2^32`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Fixed(i64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FixedError {
    Overflow,
    DivisionByZero,
}

impl fmt::Display for FixedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Overflow => f.write_str("fixed-point overflow"),
            Self::DivisionByZero => f.write_str("fixed-point division by zero"),
        }
    }
}

impl std::error::Error for FixedError {}

impl Fixed {
    pub const ZERO: Self = Self(0);
    pub const ONE: Self = Self(1 << FRAC_BITS);
    pub const MIN: Self = Self(i64::MIN);
    pub const MAX: Self = Self(i64::MAX);

    #[must_use]
    pub const fn from_bits(bits: i64) -> Self {
        Self(bits)
    }

    #[must_use]
    pub const fn to_bits(self) -> i64 {
        self.0
    }

    /// Every `i32` fits exactly.
    #[must_use]
    pub const fn from_int(n: i32) -> Self {
        Self((n as i64) << FRAC_BITS)
    }

    /// `num / den`, rounded; the way to write constants such as 0.1 without floats.
    ///
    /// # Errors
    /// `DivisionByZero` if `den` is 0, `Overflow` if the result is out of range.
    pub fn from_ratio(num: i64, den: i64) -> Result<Self, FixedError> {
        if den == 0 {
            return Err(FixedError::DivisionByZero);
        }
        narrow(div_round_half_even(
            i128::from(num) << FRAC_BITS,
            i128::from(den),
        ))
    }

    /// # Errors
    /// `Overflow` if the sum is out of range.
    pub fn checked_add(self, rhs: Self) -> Result<Self, FixedError> {
        self.0
            .checked_add(rhs.0)
            .map(Self)
            .ok_or(FixedError::Overflow)
    }

    /// # Errors
    /// `Overflow` if the difference is out of range.
    pub fn checked_sub(self, rhs: Self) -> Result<Self, FixedError> {
        self.0
            .checked_sub(rhs.0)
            .map(Self)
            .ok_or(FixedError::Overflow)
    }

    /// # Errors
    /// `Overflow` if the rounded product is out of range.
    pub fn checked_mul(self, rhs: Self) -> Result<Self, FixedError> {
        // |a * b| <= 2^126, so the full product always fits in i128.
        let product = i128::from(self.0) * i128::from(rhs.0);
        narrow(div_round_half_even(product, 1 << FRAC_BITS))
    }

    /// # Errors
    /// `DivisionByZero` if `rhs` is zero, `Overflow` if the rounded quotient is out of range.
    pub fn checked_div(self, rhs: Self) -> Result<Self, FixedError> {
        if rhs.0 == 0 {
            return Err(FixedError::DivisionByZero);
        }
        narrow(div_round_half_even(
            i128::from(self.0) << FRAC_BITS,
            i128::from(rhs.0),
        ))
    }

    /// `self * num / den` with a single rounding, e.g. a per-day rate times elapsed seconds
    /// over seconds per day. Two separate steps would round twice.
    ///
    /// # Errors
    /// `DivisionByZero` if `den` is 0, `Overflow` if the result is out of range.
    pub fn mul_ratio(self, num: i64, den: i64) -> Result<Self, FixedError> {
        if den == 0 {
            return Err(FixedError::DivisionByZero);
        }
        // |bits * num| <= 2^126, so the product always fits in i128.
        narrow(div_round_half_even(
            i128::from(self.0) * i128::from(num),
            i128::from(den),
        ))
    }
}

fn narrow(bits: i128) -> Result<Fixed, FixedError> {
    i64::try_from(bits)
        .map(Fixed)
        .map_err(|_| FixedError::Overflow)
}

/// `n / d` rounded to nearest, ties to even. `d` must be non-zero; callers check.
/// Inputs stay below 2^127 in magnitude, so nothing here can overflow.
fn div_round_half_even(n: i128, d: i128) -> i128 {
    let q = n / d;
    let r = n % d;
    if r == 0 {
        return q;
    }
    // r != 0 means n != 0, so the exact quotient has a definite sign.
    let away = if (n < 0) == (d < 0) { q + 1 } else { q - 1 };
    match (r.unsigned_abs() * 2).cmp(&d.unsigned_abs()) {
        std::cmp::Ordering::Less => q,
        std::cmp::Ordering::Greater => away,
        std::cmp::Ordering::Equal => {
            if q % 2 == 0 {
                q
            } else {
                away
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    const HALF: Fixed = Fixed(1 << 31);
    const ULP: Fixed = Fixed(1);

    fn f(n: i32) -> Fixed {
        Fixed::from_int(n)
    }

    #[test]
    fn integers_round_trip() {
        assert_eq!(f(3).to_bits(), 3 << 32);
        assert_eq!(f(-3).checked_add(f(5)), Ok(f(2)));
        assert_eq!(f(6).checked_mul(f(-7)), Ok(f(-42)));
        assert_eq!(f(42).checked_div(f(-6)), Ok(f(-7)));
    }

    #[test]
    fn ratio_builds_exact_halves() {
        assert_eq!(Fixed::from_ratio(1, 2), Ok(HALF));
        assert_eq!(Fixed::from_ratio(-3, 2), Ok(Fixed(-(3 << 31))));
    }

    #[test]
    fn mul_rounds_ties_to_even() {
        // 1 ulp * 0.5 = 0.5 ulp -> 0 (even); 3 ulp * 0.5 = 1.5 ulp -> 2 (even).
        assert_eq!(ULP.checked_mul(HALF), Ok(Fixed(0)));
        assert_eq!(Fixed(3).checked_mul(HALF), Ok(Fixed(2)));
        assert_eq!(Fixed(5).checked_mul(HALF), Ok(Fixed(2)));
        assert_eq!(Fixed(-1).checked_mul(HALF), Ok(Fixed(0)));
        assert_eq!(Fixed(-3).checked_mul(HALF), Ok(Fixed(-2)));
    }

    #[test]
    fn mul_rounds_to_nearest_off_ties() {
        // 3 ulp * 0.25 = 0.75 ulp -> 1; 1 ulp * 0.25 = 0.25 ulp -> 0.
        let quarter = Fixed(1 << 30);
        assert_eq!(Fixed(3).checked_mul(quarter), Ok(ULP));
        assert_eq!(ULP.checked_mul(quarter), Ok(Fixed(0)));
        assert_eq!(Fixed(-3).checked_mul(quarter), Ok(Fixed(-1)));
    }

    #[test]
    fn div_rounds_ties_to_even() {
        // 1 ulp / 2 = 0.5 -> 0; 3 ulp / 2 = 1.5 -> 2; -5 ulp / 2 = -2.5 -> -2.
        assert_eq!(ULP.checked_div(f(2)), Ok(Fixed(0)));
        assert_eq!(Fixed(3).checked_div(f(2)), Ok(Fixed(2)));
        assert_eq!(Fixed(-5).checked_div(f(2)), Ok(Fixed(-2)));
    }

    #[test]
    fn one_third_rounds_down_and_two_thirds_up() {
        // 2^32 / 3 = 1431655765.33.. and 2 * 2^32 / 3 = 2863311530.66..
        assert_eq!(Fixed::from_ratio(1, 3), Ok(Fixed(1_431_655_765)));
        assert_eq!(Fixed::from_ratio(2, 3), Ok(Fixed(2_863_311_531)));
    }

    #[test]
    fn overflow_is_an_error() {
        assert_eq!(Fixed::MAX.checked_add(ULP), Err(FixedError::Overflow));
        assert_eq!(Fixed::MIN.checked_sub(ULP), Err(FixedError::Overflow));
        assert_eq!(f(65_536).checked_mul(f(65_536)), Err(FixedError::Overflow));
        assert_eq!(Fixed::MAX.checked_div(HALF), Err(FixedError::Overflow));
        assert_eq!(Fixed::MIN.checked_div(f(-1)), Err(FixedError::Overflow));
        assert_eq!(Fixed::from_ratio(i64::MAX, 1), Err(FixedError::Overflow));
    }

    #[test]
    fn mul_ratio_rounds_once() {
        // 3 ulp * 1/2 = 1.5 ulp -> 2; 10 * 43200/86400 = 5 exactly.
        assert_eq!(Fixed(3).mul_ratio(1, 2), Ok(Fixed(2)));
        assert_eq!(f(10).mul_ratio(43_200, 86_400), Ok(f(5)));
        assert_eq!(f(1).mul_ratio(1, 0), Err(FixedError::DivisionByZero));
        assert_eq!(Fixed::MAX.mul_ratio(2, 1), Err(FixedError::Overflow));
    }

    #[test]
    fn division_by_zero_is_an_error() {
        assert_eq!(
            f(1).checked_div(Fixed::ZERO),
            Err(FixedError::DivisionByZero)
        );
        assert_eq!(Fixed::from_ratio(1, 0), Err(FixedError::DivisionByZero));
    }

    /// Reference rounding built a different way (floor division, then fix up) so the
    /// property tests do not just re-run the code under test.
    fn reference(n: i128, d: i128) -> Result<Fixed, FixedError> {
        let (n, d) = if d < 0 { (-n, -d) } else { (n, d) };
        let floor = n.div_euclid(d);
        let rem = n.rem_euclid(d);
        let q = match (rem * 2).cmp(&d) {
            std::cmp::Ordering::Less => floor,
            std::cmp::Ordering::Greater => floor + 1,
            std::cmp::Ordering::Equal => floor + floor.rem_euclid(2),
        };
        i64::try_from(q)
            .map(Fixed)
            .map_err(|_| FixedError::Overflow)
    }

    proptest! {
        #[test]
        fn add_is_commutative(a: i64, b: i64) {
            let (a, b) = (Fixed(a), Fixed(b));
            prop_assert_eq!(a.checked_add(b), b.checked_add(a));
        }

        #[test]
        fn add_then_sub_is_identity(a: i64, b: i64) {
            let (a, b) = (Fixed(a), Fixed(b));
            if let Ok(sum) = a.checked_add(b) {
                prop_assert_eq!(sum.checked_sub(b), Ok(a));
            }
        }

        #[test]
        fn mul_is_commutative(a: i64, b: i64) {
            let (a, b) = (Fixed(a), Fixed(b));
            prop_assert_eq!(a.checked_mul(b), b.checked_mul(a));
        }

        #[test]
        fn mul_by_one_is_identity(a: i64) {
            prop_assert_eq!(Fixed(a).checked_mul(Fixed::ONE), Ok(Fixed(a)));
            prop_assert_eq!(Fixed(a).checked_div(Fixed::ONE), Ok(Fixed(a)));
        }

        #[test]
        fn mul_matches_reference(a: i64, b: i64) {
            let expected = reference(i128::from(a) * i128::from(b), 1 << 32);
            prop_assert_eq!(Fixed(a).checked_mul(Fixed(b)), expected);
        }

        #[test]
        fn small_mul_never_overflows(
            a in -(1i64 << 46)..(1i64 << 46),
            b in -(1i64 << 46)..(1i64 << 46),
        ) {
            // Raw values below 2^46 are below 2^14 in value, so the product stays below 2^28.
            // Full-range inputs almost always overflow, so this is where rounding gets exercised.
            let got = Fixed(a).checked_mul(Fixed(b));
            prop_assert!(got.is_ok());
            prop_assert_eq!(got, reference(i128::from(a) * i128::from(b), 1 << 32));
        }

        #[test]
        fn halving_matches_reference(a: i64) {
            // Every odd `a` is an exact tie, which random full-range inputs almost never hit.
            let n = i128::from(a);
            prop_assert_eq!(Fixed(a).checked_mul(HALF), reference(n << 31, 1 << 32));
            prop_assert_eq!(Fixed(a).checked_div(f(2)), reference(n << 32, 2 << 32));
        }

        #[test]
        fn div_matches_reference(a: i64, b in any::<i64>().prop_filter("non-zero", |b| *b != 0)) {
            let expected = reference(i128::from(a) << 32, i128::from(b));
            prop_assert_eq!(Fixed(a).checked_div(Fixed(b)), expected);
        }

        #[test]
        fn mul_ratio_matches_reference(
            a: i64,
            n: i64,
            d in any::<i64>().prop_filter("non-zero", |d| *d != 0),
        ) {
            let expected = reference(i128::from(a) * i128::from(n), i128::from(d));
            prop_assert_eq!(Fixed(a).mul_ratio(n, d), expected);
        }

        #[test]
        fn ratio_matches_reference(n: i64, d in any::<i64>().prop_filter("non-zero", |d| *d != 0)) {
            let expected = reference(i128::from(n) << 32, i128::from(d));
            prop_assert_eq!(Fixed::from_ratio(n, d), expected);
        }
    }
}
