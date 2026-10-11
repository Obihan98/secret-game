//! Our own seeded PRNG (TM9): xoshiro256**, one independent stream per system.
//!
//! Written here rather than taken from a crate so a dependency update can never change the
//! rolls of a running world. Golden-value tests pin the output.

const GOLDEN_GAMMA: u64 = 0x9E37_79B9_7F4A_7C15;

/// Which system a stream belongs to (combat, events, ...). Each system owns one ID, so a
/// change in one system's draws never shifts another's.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StreamId(pub u32);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rng {
    s: [u64; 4],
}

impl Rng {
    /// The stream for one system in one world. The four state words come from `SplitMix64`, so
    /// they are never all zero (xoshiro's one bad state).
    #[must_use]
    pub fn new(world_seed: u64, stream: StreamId) -> Self {
        // `mix` is a bijection, so within a world every stream ID starts from a different point.
        let mut sm = world_seed ^ mix(u64::from(stream.0).wrapping_add(GOLDEN_GAMMA));
        Self {
            s: [
                splitmix_next(&mut sm),
                splitmix_next(&mut sm),
                splitmix_next(&mut sm),
                splitmix_next(&mut sm),
            ],
        }
    }

    pub fn next_u64(&mut self) -> u64 {
        let s = &mut self.s;
        let result = s[1].wrapping_mul(5).rotate_left(7).wrapping_mul(9);
        let t = s[1] << 17;
        s[2] ^= s[0];
        s[3] ^= s[1];
        s[1] ^= s[2];
        s[0] ^= s[3];
        s[2] ^= t;
        s[3] = s[3].rotate_left(45);
        result
    }
}

fn splitmix_next(state: &mut u64) -> u64 {
    *state = state.wrapping_add(GOLDEN_GAMMA);
    mix(*state)
}

fn mix(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

#[cfg(test)]
mod tests {
    use super::{Rng, StreamId, splitmix_next};
    use proptest::prelude::*;

    fn draws(rng: &mut Rng, n: usize) -> Vec<u64> {
        (0..n).map(|_| rng.next_u64()).collect()
    }

    // Golden values below were computed by an independent Python implementation; the first two
    // checks match the published reference outputs of each algorithm.

    #[test]
    fn xoshiro_matches_reference_output() {
        let mut rng = Rng { s: [1, 2, 3, 4] };
        assert_eq!(
            draws(&mut rng, 6),
            [
                11_520,
                0,
                1_509_978_240,
                1_215_971_899_390_074_240,
                1_216_172_134_540_287_360,
                607_988_272_756_665_600,
            ]
        );
    }

    #[test]
    fn splitmix_matches_reference_output() {
        let mut state = 1_234_567;
        let out: Vec<u64> = (0..3).map(|_| splitmix_next(&mut state)).collect();
        assert_eq!(
            out,
            [
                6_457_827_717_110_365_317,
                3_203_168_211_198_807_973,
                9_817_491_932_198_370_423,
            ]
        );
    }

    #[test]
    fn stream_golden_values() {
        let cases: [(u64, u32, [u64; 4]); 3] = [
            (
                0,
                0,
                [
                    18_110_106_563_157_542_208,
                    8_650_457_082_529_208_451,
                    3_032_169_436_225_125_478,
                    5_211_024_849_135_804_362,
                ],
            ),
            (
                42,
                0,
                [
                    2_303_456_275_738_999_573,
                    5_438_210_688_795_116_325,
                    10_433_286_269_970_717_030,
                    11_285_640_936_736_227_471,
                ],
            ),
            (
                42,
                1,
                [
                    6_361_458_328_680_127_711,
                    2_631_838_692_071_214_148,
                    13_221_152_241_774_850_206,
                    4_150_474_096_137_592_771,
                ],
            ),
        ];
        for (seed, stream, expected) in cases {
            let mut rng = Rng::new(seed, StreamId(stream));
            assert_eq!(draws(&mut rng, 4), expected, "seed {seed}, stream {stream}");
        }
    }

    proptest! {
        #[test]
        fn same_seed_same_stream_same_draws(seed: u64, stream: u32) {
            let mut a = Rng::new(seed, StreamId(stream));
            let mut b = Rng::new(seed, StreamId(stream));
            prop_assert_eq!(draws(&mut a, 64), draws(&mut b, 64));
        }

        #[test]
        fn drawing_from_one_stream_never_changes_another(
            seed: u64,
            a in any::<u32>(),
            b in any::<u32>(),
            extra in 0usize..500,
        ) {
            let mut alone = Rng::new(seed, StreamId(b));
            let expected = draws(&mut alone, 32);

            let mut busy = Rng::new(seed, StreamId(a));
            let mut quiet = Rng::new(seed, StreamId(b));
            let mut got = Vec::new();
            for i in 0..32 {
                if i % 4 == 0 {
                    draws(&mut busy, extra);
                }
                got.push(quiet.next_u64());
            }
            prop_assert_eq!(got, expected);
        }

        #[test]
        fn different_streams_differ(seed: u64, a: u32, b: u32) {
            prop_assume!(a != b);
            let mut x = Rng::new(seed, StreamId(a));
            let mut y = Rng::new(seed, StreamId(b));
            prop_assert_ne!(draws(&mut x, 4), draws(&mut y, 4));
        }
    }
}
