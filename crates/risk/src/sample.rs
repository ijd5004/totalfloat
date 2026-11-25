//! Three-point estimates and their distributions, sampled through the
//! quantile function so correlated and independent draws work the same way.
use std::collections::BTreeMap;
use std::sync::Arc;

use totalfloat_model::Minutes;

/// The shape of an estimate between its minimum and maximum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Distribution {
    /// Triangular, peaking at the most likely value.
    #[default]
    Triangle,
    /// BetaPERT: a beta distribution with mean (min + 4 × likely + max) / 6.
    Pert,
    /// Uniform between the minimum and maximum; the most likely value is ignored.
    Uniform,
}

/// A three-point estimate of an activity's remaining duration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Estimate {
    /// Optimistic.
    pub min: Minutes,
    /// Most likely.
    pub likely: Minutes,
    /// Pessimistic.
    pub max: Minutes,
    /// The shape between them.
    pub distribution: Distribution,
}

/// Points in a BetaPERT quantile table, less one.
const STEPS: usize = 1024;

/// How one activity is sampled.
#[derive(Debug, Clone)]
pub(crate) struct Sampler {
    min: f64,
    likely: f64,
    max: f64,
    shape: Shape,
}

#[derive(Debug, Clone)]
enum Shape {
    Constant,
    Uniform,
    Triangle,
    /// The standard beta quantile at `STEPS + 1` evenly spaced probabilities.
    Pert(Arc<Vec<f64>>),
}

/// Beta quantile tables by shape, built once and shared by every estimate
/// with the same one.
#[derive(Default)]
pub(crate) struct Tables(BTreeMap<(u64, u64), Arc<Vec<f64>>>);

impl Tables {
    fn beta(&mut self, a: f64, b: f64) -> Arc<Vec<f64>> {
        self.0
            .entry((a.to_bits(), b.to_bits()))
            .or_insert_with(|| Arc::new(beta_table(a, b)))
            .clone()
    }
}

#[allow(
    clippy::cast_precision_loss,
    reason = "durations in minutes are far below 2^52"
)]
fn float(m: Minutes) -> f64 {
    m.get() as f64
}

impl Sampler {
    pub(crate) fn new(estimate: &Estimate, tables: &mut Tables) -> Self {
        let (min, likely, max) = (
            float(estimate.min),
            float(estimate.likely),
            float(estimate.max),
        );
        let shape = if max <= min {
            Shape::Constant
        } else {
            match estimate.distribution {
                Distribution::Uniform => Shape::Uniform,
                Distribution::Triangle => Shape::Triangle,
                Distribution::Pert => {
                    let a = 1.0 + 4.0 * (likely - min) / (max - min);
                    let b = 1.0 + 4.0 * (max - likely) / (max - min);
                    Shape::Pert(tables.beta(a, b))
                }
            }
        };
        Self {
            min,
            likely,
            max,
            shape,
        }
    }

    /// The duration at probability `u` in (0, 1), to the nearest minute.
    #[allow(
        clippy::cast_possible_truncation,
        reason = "rounded, and within the estimate's own range"
    )]
    #[allow(
        clippy::many_single_char_names,
        reason = "the triangle's usual notation: a, c, b"
    )]
    pub(crate) fn at(&self, u: f64) -> Minutes {
        let (a, c, b) = (self.min, self.likely, self.max);
        let x = match &self.shape {
            Shape::Constant => a,
            Shape::Uniform => a + u * (b - a),
            Shape::Triangle => {
                let split = (c - a) / (b - a);
                if u < split {
                    a + (u * (b - a) * (c - a)).sqrt()
                } else {
                    b - ((1.0 - u) * (b - a) * (b - c)).sqrt()
                }
            }
            Shape::Pert(table) => a + (b - a) * interpolate(table, u),
        };
        Minutes::new(x.round().clamp(a, b) as i64)
    }
}

#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a table index from a probability in (0, 1)"
)]
fn interpolate(table: &[f64], u: f64) -> f64 {
    let position = u * STEPS as f64;
    let i = (position.floor() as usize).min(STEPS - 1);
    let t = position - i as f64;
    table[i] + t * (table[i + 1] - table[i])
}

/// Quantiles of the standard beta distribution, by bisection on its
/// distribution function.
#[allow(clippy::cast_precision_loss, reason = "a small table index")]
fn beta_table(a: f64, b: f64) -> Vec<f64> {
    let ln_beta = ln_gamma(a) + ln_gamma(b) - ln_gamma(a + b);
    (0..=STEPS)
        .map(|i| {
            let p = i as f64 / STEPS as f64;
            if i == 0 {
                return 0.0;
            }
            if i == STEPS {
                return 1.0;
            }
            let (mut low, mut high) = (0.0_f64, 1.0_f64);
            for _ in 0..60 {
                let mid = f64::midpoint(low, high);
                if incomplete_beta(mid, a, b, ln_beta) < p {
                    low = mid;
                } else {
                    high = mid;
                }
            }
            f64::midpoint(low, high)
        })
        .collect()
}

/// The regularized incomplete beta function, from its continued fraction.
#[allow(
    clippy::many_single_char_names,
    reason = "the continued fraction's usual notation"
)]
fn incomplete_beta(x: f64, a: f64, b: f64, ln_beta: f64) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    if x >= 1.0 {
        return 1.0;
    }
    if x > (a + 1.0) / (a + b + 2.0) {
        return 1.0 - incomplete_beta(1.0 - x, b, a, ln_beta);
    }
    let front = (a * x.ln() + b * (1.0 - x).ln() - ln_beta).exp() / a;
    // Modified Lentz evaluation of 1 / (1 + d1 / (1 + d2 / (1 + ...))).
    let tiny = 1e-300;
    let (mut c, mut d) = (1.0, 1.0 - (a + b) * x / (a + 1.0));
    if d.abs() < tiny {
        d = tiny;
    }
    d = 1.0 / d;
    let mut f = d;
    for m in 1..300 {
        let m = f64::from(m);
        let even = m * (b - m) * x / ((a + 2.0 * m - 1.0) * (a + 2.0 * m));
        let odd = -(a + m) * (a + b + m) * x / ((a + 2.0 * m) * (a + 2.0 * m + 1.0));
        for term in [even, odd] {
            d = 1.0 + term * d;
            if d.abs() < tiny {
                d = tiny;
            }
            c = 1.0 + term / c;
            if c.abs() < tiny {
                c = tiny;
            }
            d = 1.0 / d;
            f *= d * c;
        }
        if (d * c - 1.0).abs() < 1e-15 {
            break;
        }
    }
    front * f
}

/// The log of the gamma function (Lanczos, g = 7).
fn ln_gamma(x: f64) -> f64 {
    const G: [f64; 9] = [
        0.999_999_999_999_809_9,
        676.520_368_121_885_1,
        -1_259.139_216_722_402_8,
        771.323_428_777_653_1,
        -176.615_029_162_140_6,
        12.507_343_278_686_905,
        -0.138_571_095_265_720_12,
        9.984_369_578_019_572e-6,
        1.505_632_735_149_311_6e-7,
    ];
    let x = x - 1.0;
    let mut sum = G[0];
    for (i, g) in G.iter().enumerate().skip(1) {
        sum += g / (x + f64::from(u32::try_from(i).unwrap_or(0)));
    }
    let t = x + 7.5;
    0.5 * (2.0 * std::f64::consts::PI).ln() + (x + 0.5) * t.ln() - t + sum.ln()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::random::Rng;

    fn estimate(min: i64, likely: i64, max: i64, distribution: Distribution) -> Estimate {
        Estimate {
            min: Minutes::new(min),
            likely: Minutes::new(likely),
            max: Minutes::new(max),
            distribution,
        }
    }

    /// The mean and bounds of 100,000 draws.
    #[allow(clippy::cast_precision_loss, reason = "test arithmetic")]
    fn draws(e: &Estimate) -> (f64, i64, i64) {
        let s = Sampler::new(e, &mut Tables::default());
        let mut r = Rng::for_iteration(7, 0);
        let all: Vec<i64> = (0..100_000).map(|_| s.at(r.uniform()).get()).collect();
        let mean = all.iter().sum::<i64>() as f64 / all.len() as f64;
        (mean, *all.iter().min().unwrap(), *all.iter().max().unwrap())
    }

    #[test]
    fn each_distribution_has_its_mean_and_stays_in_range() {
        // Means: triangle (a + c + b) / 3, PERT (a + 4c + b) / 6, uniform (a + b) / 2.
        for (distribution, mean) in [
            (Distribution::Triangle, (600.0 + 1_200.0 + 3_000.0) / 3.0),
            (Distribution::Pert, (600.0 + 4.0 * 1_200.0 + 3_000.0) / 6.0),
            (Distribution::Uniform, f64::midpoint(600.0, 3_000.0)),
        ] {
            let (got, low, high) = draws(&estimate(600, 1_200, 3_000, distribution));
            assert!(
                (got - mean).abs() < 0.01 * mean,
                "{distribution:?}: {got} vs {mean}"
            );
            assert!(
                low >= 600 && high <= 3_000,
                "{distribution:?}: {low}..{high}"
            );
        }
    }

    #[test]
    fn a_collapsed_estimate_is_constant() {
        let (mean, low, high) = draws(&estimate(480, 480, 480, Distribution::Pert));
        assert_eq!((low, high), (480, 480));
        assert!((mean - 480.0).abs() < f64::EPSILON);
    }

    #[test]
    fn the_triangle_quantile_is_exact() {
        // a = 0, c = 100, b = 400: F(c) = 0.25, and the median solves
        // 1 - (400 - x)^2 / (400 × 300) = 0.5, x = 400 - sqrt(60,000).
        let s = Sampler::new(
            &estimate(0, 100, 400, Distribution::Triangle),
            &mut Tables::default(),
        );
        assert_eq!(s.at(0.25).get(), 100);
        assert_eq!(s.at(0.5).get(), 155);
    }

    #[test]
    fn the_beta_quantile_matches_known_values() {
        // Beta(2, 2) is symmetric with median 0.5; Beta(1, 1) is uniform;
        // Beta(2, 1) has distribution function x^2.
        let close = |a: f64, b: f64| (a - b).abs() < 1e-6;
        assert!(close(interpolate(&beta_table(2.0, 2.0), 0.5), 0.5));
        assert!(close(interpolate(&beta_table(1.0, 1.0), 0.3), 0.3));
        assert!(close(interpolate(&beta_table(2.0, 1.0), 0.25), 0.5));
        assert!((ln_gamma(5.0) - 24f64.ln()).abs() < 1e-12);
    }
}
