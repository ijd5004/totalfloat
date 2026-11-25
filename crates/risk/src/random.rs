//! A small seeded generator and the normal distribution, so results depend
//! on nothing but the seed.
//!
//! The generator is xoshiro256** (Blackman and Vigna, public domain), seeded
//! through splitmix64. The normal quantile is Acklam's rational approximation,
//! refined by one Halley step, and the normal distribution function comes from
//! the complementary error function's continued fraction and series.

/// xoshiro256**.
pub(crate) struct Rng {
    state: [u64; 4],
}

/// One step of splitmix64, which spreads a seed over the whole state.
fn splitmix(x: &mut u64) -> u64 {
    *x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *x;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

impl Rng {
    /// The stream for one iteration of a run: the same `(seed, iteration)`
    /// always gives the same numbers, whichever thread draws them.
    pub(crate) fn for_iteration(seed: u64, iteration: u64) -> Self {
        // The seed is spread first, so a seed one iteration-step from another
        // doesn't replay its streams.
        let mut spread = seed;
        let mut x = splitmix(&mut spread) ^ iteration.wrapping_mul(0xD1B5_4A32_D192_ED03);
        let state = [
            splitmix(&mut x),
            splitmix(&mut x),
            splitmix(&mut x),
            splitmix(&mut x),
        ];
        Self { state }
    }

    fn next(&mut self) -> u64 {
        let s = &mut self.state;
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

    /// Uniform on the open interval (0, 1).
    #[allow(clippy::cast_precision_loss, reason = "53 bits, exactly representable")]
    pub(crate) fn uniform(&mut self) -> f64 {
        ((self.next() >> 11) as f64 + 0.5) / (1u64 << 53) as f64
    }

    /// Standard normal.
    pub(crate) fn normal(&mut self) -> f64 {
        normal_quantile(self.uniform())
    }
}

/// The standard normal distribution function.
pub(crate) fn normal_cdf(z: f64) -> f64 {
    0.5 * erfc(-z / std::f64::consts::SQRT_2)
}

/// The complementary error function: a series for small arguments and a
/// continued fraction for large ones, both to near double precision.
fn erfc(x: f64) -> f64 {
    if x < 0.0 {
        return 2.0 - erfc(-x);
    }
    if x < 2.0 {
        // erf(x) = 2/sqrt(pi) * sum (-1)^n x^(2n+1) / (n! (2n+1))
        let mut term = x;
        let mut sum = x;
        let x2 = x * x;
        for n in 1..60 {
            let n = f64::from(n);
            term *= -x2 / n;
            let next = term / (2.0 * n + 1.0);
            sum += next;
            if next.abs() < 1e-17 * sum.abs() {
                break;
            }
        }
        return 1.0 - sum * 2.0 / std::f64::consts::PI.sqrt();
    }
    // erfc(x) = exp(-x^2)/sqrt(pi) * 1/(x + 1/2/(x + 1/(x + 3/2/(x + ...))))
    // evaluated from the tail with 60 terms.
    let mut fraction = 0.0;
    for k in (1..=60).rev() {
        fraction = f64::from(k) / 2.0 / (x + fraction);
    }
    (-x * x).exp() / std::f64::consts::PI.sqrt() / (x + fraction)
}

/// The standard normal quantile.
pub(crate) fn normal_quantile(p: f64) -> f64 {
    const A: [f64; 6] = [
        -3.969_683_028_665_376e1,
        2.209_460_984_245_205e2,
        -2.759_285_104_469_687e2,
        1.383_577_518_672_69e2,
        -3.066_479_806_614_716e1,
        2.506_628_277_459_239,
    ];
    const B: [f64; 5] = [
        -5.447_609_879_822_406e1,
        1.615_858_368_580_409e2,
        -1.556_989_798_598_866e2,
        6.680_131_188_771_972e1,
        -1.328_068_155_288_572e1,
    ];
    const C: [f64; 6] = [
        -7.784_894_002_430_293e-3,
        -3.223_964_580_411_365e-1,
        -2.400_758_277_161_838,
        -2.549_732_539_343_734,
        4.374_664_141_464_968,
        2.938_163_982_698_783,
    ];
    const D: [f64; 4] = [
        7.784_695_709_041_462e-3,
        3.224_671_290_700_398e-1,
        2.445_134_137_142_996,
        3.754_408_661_907_416,
    ];
    let low = 0.024_25;
    let x = if p < low {
        let q = (-2.0 * p.ln()).sqrt();
        (((((C[0] * q + C[1]) * q + C[2]) * q + C[3]) * q + C[4]) * q + C[5])
            / ((((D[0] * q + D[1]) * q + D[2]) * q + D[3]) * q + 1.0)
    } else if p <= 1.0 - low {
        let q = p - 0.5;
        let r = q * q;
        (((((A[0] * r + A[1]) * r + A[2]) * r + A[3]) * r + A[4]) * r + A[5]) * q
            / (((((B[0] * r + B[1]) * r + B[2]) * r + B[3]) * r + B[4]) * r + 1.0)
    } else {
        let q = (-2.0 * (1.0 - p).ln()).sqrt();
        -(((((C[0] * q + C[1]) * q + C[2]) * q + C[3]) * q + C[4]) * q + C[5])
            / ((((D[0] * q + D[1]) * q + D[2]) * q + D[3]) * q + 1.0)
    };
    // One Halley step brings it to full precision.
    let e = normal_cdf(x) - p;
    let u = e * (2.0 * std::f64::consts::PI).sqrt() * (x * x / 2.0).exp();
    x - u / (1.0 + x * u / 2.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_normal_matches_known_values() {
        let close = |a: f64, b: f64| (a - b).abs() < 1e-9;
        assert!(close(normal_cdf(0.0), 0.5));
        assert!(close(normal_cdf(1.0), 0.841_344_746_068_542_9));
        assert!(close(normal_cdf(-1.959_963_984_540_054), 0.025));
        assert!(close(normal_cdf(3.0), 0.998_650_101_968_369_9));
        assert!(close(normal_quantile(0.975), 1.959_963_984_540_054));
        assert!(close(normal_quantile(0.5), 0.0));
        assert!(close(normal_quantile(0.001), -3.090_232_306_167_813_5));
        for p in [1e-6, 0.01, 0.2, 0.5, 0.8, 0.99, 1.0 - 1e-6] {
            assert!((normal_cdf(normal_quantile(p)) - p).abs() < 1e-12, "{p}");
        }
    }

    #[test]
    fn a_stream_is_fixed_by_its_seed_and_iteration() {
        let draw = |seed, i| {
            let mut r = Rng::for_iteration(seed, i);
            (0..4).map(|_| r.uniform().to_bits()).collect::<Vec<_>>()
        };
        assert_eq!(draw(1, 7), draw(1, 7));
        assert_ne!(draw(1, 7), draw(1, 8));
        assert_ne!(draw(1, 7), draw(2, 7));
        // A seed one iteration-step away doesn't replay another's stream
        //.
        assert_ne!(draw(0, 1), draw(0xD1B5_4A32_D192_ED03, 0));
        let mut r = Rng::for_iteration(42, 0);
        let mean = (0..100_000).map(|_| r.uniform()).sum::<f64>() / 100_000.0;
        assert!((mean - 0.5).abs() < 0.005, "{mean}");
    }
}
