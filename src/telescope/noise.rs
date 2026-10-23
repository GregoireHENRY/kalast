//! Random numbers for a frame's noise: uniform, normal and Poisson.
//!
//! kalast's own rather than `rand`'s, so that a seed gives the same frames
//! in every version: `rand` documents its `StdRng` as free to change
//! algorithm between releases, and a night re-made from its seed should be
//! the night it was. xoshiro256++ (Blackman & Vigna 2018), seeded through
//! SplitMix64 as its authors advise.

/// xoshiro256++: fast, 256 bits of state, passes BigCrush.
#[derive(Debug, Clone)]
pub struct Rng {
    s: [u64; 4],
}

impl Rng {
    /// The generator for `seed`, any value, zero included.
    pub fn new(seed: u64) -> Self {
        let mut z = seed;
        let mut next = || {
            z = z.wrapping_add(0x9e37_79b9_7f4a_7c15);
            let mut x = z;
            x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
            x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
            x ^ (x >> 31)
        };
        Self { s: [next(), next(), next(), next()] }
    }

    pub fn next_u64(&mut self) -> u64 {
        let s = &mut self.s;
        let out = s[0].wrapping_add(s[3]).rotate_left(23).wrapping_add(s[0]);
        let t = s[1] << 17;
        s[2] ^= s[0];
        s[3] ^= s[1];
        s[1] ^= s[2];
        s[0] ^= s[3];
        s[2] ^= t;
        s[3] = s[3].rotate_left(45);
        out
    }

    /// Uniform in [0, 1), 53 bits.
    pub fn uniform(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
    }

    /// Standard normal: Marsaglia's polar method.
    pub fn normal(&mut self) -> f64 {
        loop {
            let u = 2.0 * self.uniform() - 1.0;
            let v = 2.0 * self.uniform() - 1.0;
            let s = u * u + v * v;
            if s > 0.0 && s < 1.0 {
                return u * (-2.0 * s.ln() / s).sqrt();
            }
        }
    }

    /// Poisson with mean `mu`, exactly: by inversion under 10, by Hörmann's
    /// transformed rejection (PTRS, 1993) above, which costs about one
    /// uniform pair whatever the mean. `0` for a mean that is not positive.
    pub fn poisson(&mut self, mu: f64) -> f64 {
        if !(mu > 0.0) {
            return 0.0;
        }
        if mu < 10.0 {
            let limit = (-mu).exp();
            let mut k = 0.0;
            let mut p = self.uniform();
            while p > limit {
                k += 1.0;
                p *= self.uniform();
            }
            return k;
        }
        let smu = mu.sqrt();
        let b = 0.931 + 2.53 * smu;
        let a = -0.059 + 0.02483 * b;
        let inv_alpha = 1.1239 + 1.1328 / (b - 3.4);
        let vr = 0.9277 - 3.6224 / (b - 2.0);
        let log_mu = mu.ln();
        loop {
            let u = self.uniform() - 0.5;
            let v = self.uniform();
            let us = 0.5 - u.abs();
            let k = ((2.0 * a / us + b) * u + mu + 0.43).floor();
            if us >= 0.07 && v <= vr {
                return k;
            }
            if k < 0.0 || (us < 0.013 && v > us) {
                continue;
            }
            if v.ln() + inv_alpha.ln() - (a / (us * us) + b).ln() <= -mu + k * log_mu - ln_gamma(k + 1.0) {
                return k;
            }
        }
    }
}

/// ln Γ(x) for x > 0: Lanczos (g = 7, nine terms), good to about 1e-15.
pub fn ln_gamma(x: f64) -> f64 {
    const G: f64 = 7.0;
    const C: [f64; 9] = [
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
    if x < 0.5 {
        // Reflection: Γ(x) Γ(1 - x) = π / sin(π x).
        let pi = std::f64::consts::PI;
        return (pi / (pi * x).sin()).ln() - ln_gamma(1.0 - x);
    }
    let x = x - 1.0;
    let mut sum = C[0];
    for (i, c) in C.iter().enumerate().skip(1) {
        sum += c / (x + i as f64);
    }
    let t = x + G + 0.5;
    0.5 * (2.0 * std::f64::consts::PI).ln() + (x + 0.5) * t.ln() - t + sum.ln()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn moments(mut draw: impl FnMut() -> f64, n: usize) -> (f64, f64) {
        let (mut s, mut s2) = (0.0, 0.0);
        for _ in 0..n {
            let x = draw();
            s += x;
            s2 += x * x;
        }
        let mean = s / n as f64;
        (mean, s2 / n as f64 - mean * mean)
    }

    #[test]
    fn a_seed_gives_the_same_numbers() {
        let (mut a, mut b) = (Rng::new(42), Rng::new(42));
        for _ in 0..1000 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
        assert_ne!(Rng::new(1).next_u64(), Rng::new(2).next_u64());
    }

    #[test]
    fn uniform_and_normal_have_their_moments() {
        let mut r = Rng::new(7);
        let (m, v) = moments(|| r.uniform(), 1_000_000);
        assert!((m - 0.5).abs() < 2e-3 && (v - 1.0 / 12.0).abs() < 1e-3, "{m} {v}");
        let (m, v) = moments(|| r.normal(), 1_000_000);
        assert!(m.abs() < 5e-3 && (v - 1.0).abs() < 5e-3, "{m} {v}");
    }

    #[test]
    fn poisson_has_its_mean_and_variance_on_both_branches() {
        let mut r = Rng::new(11);
        for mu in [0.3, 3.0, 9.9, 10.0, 47.5, 1_000.0, 1.0e6] {
            let (m, v) = moments(|| r.poisson(mu), 400_000);
            // Five standard errors of the mean and of the variance.
            assert!((m - mu).abs() < 5.0 * (mu / 4e5).sqrt(), "mu {mu}: mean {m}");
            assert!((v - mu).abs() < 5.0 * mu * (2.0 / 4e5f64).sqrt() + 1e-3, "mu {mu}: variance {v}");
        }
        assert_eq!(r.poisson(0.0), 0.0);
    }

    #[test]
    fn poisson_gives_the_right_probabilities() {
        // P(k) for mu = 25, against exp(-mu) mu^k / k!, where PTRS samples.
        let (mu, n) = (25.0, 2_000_000);
        let mut r = Rng::new(3);
        let mut counts = [0usize; 60];
        for _ in 0..n {
            let k = r.poisson(mu) as usize;
            if k < counts.len() {
                counts[k] += 1;
            }
        }
        for k in 15..36 {
            let p = (-mu + k as f64 * f64::ln(mu) - ln_gamma(k as f64 + 1.0)).exp();
            let seen = counts[k] as f64 / n as f64;
            assert!((seen - p).abs() < 5.0 * (p / n as f64).sqrt(), "k {k}: {seen} against {p}");
        }
    }

    #[test]
    fn ln_gamma_is_factorials() {
        let mut f = 1.0f64;
        for k in 1..30 {
            f *= k as f64;
            assert!((ln_gamma(k as f64 + 1.0) - f.ln()).abs() < 1e-12 * f.ln().max(1.0), "{k}");
        }
        assert!((ln_gamma(0.5) - std::f64::consts::PI.sqrt().ln()).abs() < 1e-14);
    }
}
