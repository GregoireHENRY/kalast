//! One step of every column at once, by the time scheme of one's choice.
//!
//! `core`'s `flux_bc`, `heat_conduction` and `bottom_flux` are one explicit
//! step in three calls. A `Stepper` takes that step in one call, or takes it
//! by another scheme: more stages of the same explicit step (RKC), or an
//! implicit one (backward Euler, Crank-Nicolson, BDF2), whose time step the
//! thinnest layer no longer bounds.
//!
//! The columns are what `core` steps. Row 0 of the temperatures is the
//! surface, which holds no heat of its own and is set by a balance; the last
//! row is the base, set by the heat flow through it; the rows between are
//! the layers, each with
//!
//! ```text
//! rho c_i dT_i/dt  =  [ k_{i+1/2} (T_{i+1} - T_i) - k_{i-1/2} (T_i - T_{i-1}) ] / dz^2
//! ```
//!
//! `k` at a boundary the harmonic mean of the two layers. With a `Ground`
//! every layer and facet has its own properties, a graded column carries its
//! layers' widths in them, and its `Laws` change them with temperature: they
//! are read at the temperatures of the step before (at a stage's own, for
//! RKC).
//!
//! # The implicit schemes
//!
//! A column is a tridiagonal system, solved facet by facet. The surface
//! balance is not linear -- it radiates as `T^4` -- and the layers under it
//! are unknowns of the same step, so it cannot be solved first as the
//! explicit step does. The system is eliminated from the base up, which
//! leaves each layer's change as a line of the change of the one above it,
//!
//! ```text
//! d_i  =  P_i d_{i-1} + S_i,        d_i = T_i' - T_i
//! ```
//!
//! and with them the surface's gradient `-3 T_0 + 4 T_1 - T_2` as a line of
//! `T_0'` alone. The balance is then one equation in one unknown, solved by
//! Newton's method, and the layers follow from the top down. Surface and
//! layers are thus at the same time level, in one elimination a step.
//! Stopped after one iteration, Newton's method is the balance linearised
//! about the temperature of the step before: the semi-implicit form of
//! Schorghofer & Khatiwala (2024).
//!
//! # Single precision
//!
//! A float32 at 300 K is 3e-5 K wide. A deep layer changes by less than that
//! at a step, and a system solved for the temperatures themselves rounds
//! each of them at every step: over thousands of steps the base of a column
//! drifted by tenths of a kelvin. So the implicit schemes and RKC solve for
//! the changes `d`, which are small numbers held to seven digits of their
//! own, and add them to the temperatures with what the sum cannot hold kept
//! for the next step (a compensated sum, as `core`'s `Remainder`).
//!
//! # Which
//!
//! `Explicit` is first order in time and stable while `D dt / dz^2 <= 1/2`.
//! `Rkc` is that step in `stages` stages, stable `~ stages^2` times further
//! for `stages` times the work, still first order. `BackwardEuler` is first
//! order and damps what it cannot resolve; `CrankNicolson` is second order
//! but only flips the sign of what it cannot resolve, which shows after a
//! surface temperature is stepped and not after a flux is (the surface is
//! solved again at every step); `Bdf2` is second order and damps, and takes
//! one backward-Euler step to start.

use anyhow::{Result, anyhow, bail};
use ndarray::{ArrayView1, ArrayViewMut2};

use super::core::{self, GroundView, Laws, Newton, RowBuf, Thermal, harmonic, newton_balance, row_of};
use crate::Float;

/// How far inside the unit circle the RKC stages' polynomial is pulled, so
/// that it stays stable between the points where it would touch the circle
/// (Verwer's value).
pub const RKC_DAMPING: f64 = 2.0 / 13.0;

/// The largest ratio of a step to the one before that BDF2 takes; past it
/// the step is a backward-Euler one. Its stable limit is `1 + sqrt(2)`.
pub(super) const BDF2_LARGEST_RATIO: Float = 2.4;

/// How a `Stepper` advances the columns in time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scheme {
    /// Forward Euler: `core`'s step.
    Explicit,
    /// Runge-Kutta-Chebyshev, first order: `stages` explicit stages a step.
    Rkc { stages: usize },
    BackwardEuler,
    CrankNicolson,
    Bdf2,
}

impl Scheme {
    pub const NAMES: [&'static str; 5] = ["explicit", "rkc", "backward-euler", "crank-nicolson", "bdf2"];

    pub fn name(&self) -> &'static str {
        match self {
            Self::Explicit => "explicit",
            Self::Rkc { .. } => "rkc",
            Self::BackwardEuler => "backward-euler",
            Self::CrankNicolson => "crank-nicolson",
            Self::Bdf2 => "bdf2",
        }
    }

    /// A scheme by its name; `stages` is read for `rkc` alone.
    pub fn named(name: &str, stages: usize) -> Result<Self> {
        Ok(match name {
            "explicit" => Self::Explicit,
            "rkc" => {
                Rkc::new(stages)?;
                Self::Rkc { stages }
            }
            "backward-euler" => Self::BackwardEuler,
            "crank-nicolson" => Self::CrankNicolson,
            "bdf2" => Self::Bdf2,
            other => bail!("no scheme named {other:?}: one of {}", Self::NAMES.join(", ")),
        })
    }

    /// Whether the scheme solves a system at each step.
    pub fn is_implicit(&self) -> bool {
        matches!(self, Self::BackwardEuler | Self::CrankNicolson | Self::Bdf2)
    }

    /// How far `stages` stages of RKC stay stable: while `dt` times the
    /// largest rate of the columns is under this. One stage is forward
    /// Euler, 2; it grows as `stages^2`.
    pub fn rkc_stability(stages: usize) -> Result<Float> {
        Ok(Rkc::new(stages)?.beta as Float)
    }

    /// The fewest RKC stages that take a step `speedup` times the largest
    /// that forward Euler takes (`stability_maxdt`).
    pub fn rkc_stages(speedup: Float) -> Result<usize> {
        if !(speedup > 0.0 && speedup.is_finite()) {
            bail!("a step {speedup} times forward Euler's: it must be positive");
        }
        for stages in 1..=Rkc::MOST {
            if Rkc::new(stages)?.beta >= 2.0 * speedup as f64 {
                return Ok(stages);
            }
        }
        bail!("a step {speedup} times forward Euler's is past {} RKC stages: an implicit scheme takes it", Rkc::MOST)
    }
}

/// The recursion of `stages` damped RKC stages of first order,
///
/// ```text
/// Y_1 = Y_0 + first dt F(Y_0)
/// Y_j = mu_j Y_{j-1} + nu_j Y_{j-2} + tilde_j dt F(Y_{j-1})
/// ```
///
/// whose stability polynomial is the Chebyshev one `T_s(w0 + w1 z) /
/// T_s(w0)`, within the unit circle for `z` from `-beta` to 0. `mu_j + nu_j
/// = 1`, so the same recursion holds for the stages' changes `Y_j - Y_0`.
pub(super) struct Rkc {
    pub(super) first: f64,
    pub(super) mu: Vec<f64>,
    pub(super) nu: Vec<f64>,
    pub(super) tilde: Vec<f64>,
    pub(super) beta: f64,
}

impl Rkc {
    /// Past this the recursion loses its precision before it gains a step.
    const MOST: usize = 200;

    pub(super) fn new(stages: usize) -> Result<Self> {
        if !(1..=Self::MOST).contains(&stages) {
            bail!("rkc takes 1 to {} stages, not {stages} (Scheme::rkc_stages gives the fewest for a step)", Self::MOST);
        }
        let s = stages;
        let w0 = 1.0 + RKC_DAMPING / (s * s) as f64;
        // T_j(w0) and its derivative, by the three-term recursion.
        let (mut t, mut d) = (vec![1.0; s + 1], vec![0.0; s + 1]);
        t[1] = w0;
        d[1] = 1.0;
        for j in 2..=s {
            t[j] = 2.0 * w0 * t[j - 1] - t[j - 2];
            d[j] = 2.0 * t[j - 1] + 2.0 * w0 * d[j - 1] - d[j - 2];
        }
        let w1 = t[s] / d[s];
        let (mut mu, mut nu, mut tilde) = (vec![0.0; s + 1], vec![0.0; s + 1], vec![0.0; s + 1]);
        for j in 2..=s {
            mu[j] = 2.0 * w0 * t[j - 1] / t[j];
            nu[j] = -t[j - 2] / t[j];
            tilde[j] = 2.0 * w1 * t[j - 1] / t[j];
        }
        // |T_s(x)| <= T_s(w0) for |x| <= w0: x = w0 + w1 z down to -w0.
        Ok(Self { first: w1 / w0, mu, nu, tilde, beta: 2.0 * w0 / w1 })
    }
}

/// What sets the surface of each column at a step.
#[derive(Clone, Copy)]
pub enum Surface<'a> {
    /// The flux each facet absorbs (W/m2): the surface is solved for the
    /// balance of it, what it radiates and what it conducts down.
    Flux(ArrayView1<'a, Float>),
    /// The temperature of each facet's surface (K), given. An explicit step
    /// goes from the surface as it is at the step's start; an implicit one
    /// to the surface as it is at its end.
    Temperature(ArrayView1<'a, Float>),
}

/// The step before, for BDF2.
struct History {
    t: Vec<Float>,
    dt: Float,
}

/// Steps the columns of one body by a `Scheme`.
///
/// One `Stepper` a body: it keeps what its steps could not add to the
/// body's temperatures, BDF2 the temperatures of the step before, and every
/// scheme its working arrays, the size of the columns.
pub struct Stepper {
    pub scheme: Scheme,
    /// How far Newton's method takes the surface balance. One iteration is
    /// the semi-implicit form.
    pub newton: Newton,
    /// The columns' shape that the arrays below are for.
    shape: (usize, usize),
    history: Option<History>,
    spare: Vec<Float>,
    /// What the steps so far could not add to each temperature.
    rest: Vec<Float>,
    p: Vec<Float>,
    s: Vec<Float>,
    y: [Vec<Float>; 3],
}

impl Stepper {
    pub fn new(scheme: Scheme) -> Self {
        Self {
            scheme,
            newton: Newton::default(),
            shape: (0, 0),
            history: None,
            spare: Vec::new(),
            rest: Vec::new(),
            p: Vec::new(),
            s: Vec::new(),
            y: [Vec::new(), Vec::new(), Vec::new()],
        }
    }

    /// Forget the steps before: BDF2 starts again with a backward-Euler
    /// step, and nothing is carried over. After the temperatures were set
    /// from outside, or after a jump that two steps should not span.
    pub fn reset(&mut self) {
        self.history = None;
        self.rest.fill(0.0);
    }

    /// One step of `dt` (s) of every column of `t`, laid out as
    /// `core::columns` makes it, its layers `dz` (m) apart: the surface from
    /// `surface`, the layers by conduction, the base from the heat flow
    /// `bottom` (W/m2) that enters it from below, 0 for none.
    ///
    /// `Explicit` is `flux_bc`, `heat_conduction` and `bottom_flux`, to the
    /// bit. An error leaves `t` as it was, except from the explicit scheme
    /// past its stability, which stops at the first layer past it.
    pub fn step<'a>(
        &mut self,
        mut t: ArrayViewMut2<'_, Float>,
        surface: Surface<'_>,
        thermal: impl Into<Thermal<'a>>,
        dt: Float,
        dz: Float,
        bottom: Float,
    ) -> Result<()> {
        let thermal = thermal.into();
        let (layers, facets) = t.dim();
        if layers < 3 {
            bail!("a step needs 3 layers or more, got {layers}");
        }
        if !(dt > 0.0 && dt.is_finite() && dz > 0.0 && dz.is_finite()) {
            bail!("a step of {dt} s through layers {dz} m apart: both must be positive");
        }
        if !bottom.is_finite() {
            bail!("the heat flow at the bottom is {bottom} W/m2: it must be finite");
        }
        let (given, what) = match &surface {
            Surface::Flux(v) => (v.len(), "absorbed fluxes"),
            Surface::Temperature(v) => (v.len(), "surface temperatures"),
        };
        if given != facets {
            bail!("{given} {what} for {facets} facets");
        }
        match &surface {
            Surface::Flux(q) => core::check_flux(*q)?,
            Surface::Temperature(v) => {
                if let Some((i, v)) = v.indexed_iter().find(|(_, v)| !(v.is_finite() && **v > 0.0)) {
                    bail!("the surface temperature of facet {i} is {v} K: it must be finite and positive");
                }
            }
        }
        match &thermal {
            Thermal::Ground(g) => g.check(layers, facets)?,
            Thermal::Body(prop) => {
                if !(prop.diffusivity > 0.0 && prop.conductivity > 0.0) {
                    bail!(
                        "the diffusivity is {} and the conductivity {}: compute them from the thermal inertia first, \
                         with compute_conductivity_diffusivity()",
                        prop.diffusivity,
                        prop.conductivity
                    );
                }
            }
        }
        if self.scheme == Scheme::Explicit {
            match &surface {
                Surface::Flux(q) => core::flux_bc_with(t.view_mut(), *q, thermal, dz, self.newton)?,
                Surface::Temperature(v) => t.row_mut(0).assign(v),
            }
            core::heat_conduction(t.view_mut(), thermal, dt, dz)?;
            return core::bottom_flux(t, bottom, thermal, dz);
        }
        // The other schemes: on the temperatures as one slice, a layer
        // after another.
        if self.shape != (layers, facets) {
            self.shape = (layers, facets);
            self.history = None;
            self.rest.clear();
            self.rest.resize(layers * facets, 0.0);
        }
        if t.as_slice_mut().is_some() {
            let flat = t.as_slice_mut().expect("asked");
            self.advance(flat, &surface, thermal, dt, dz, bottom)
        } else {
            // A view with gaps between its values: stepped in an array of
            // its own.
            let mut own = t.as_standard_layout().into_owned();
            self.advance(own.as_slice_mut().expect("in order"), &surface, thermal, dt, dz, bottom)?;
            t.assign(&own);
            Ok(())
        }
    }

    fn advance(&mut self, t: &mut [Float], surface: &Surface<'_>, thermal: Thermal<'_>, dt: Float, dz: Float, bottom: Float) -> Result<()> {
        match self.scheme {
            Scheme::Rkc { stages } => self.rkc(t, surface, thermal, dt, dz, bottom, stages),
            _ => self.implicit(t, surface, thermal, dt, dz, bottom),
        }
    }

    /// One RKC step: the explicit step's stages, the surface and the base
    /// set again at each, on the stages' changes from the step's start.
    /// `F` is taken at each stage's own temperatures, a ground's laws with
    /// them.
    #[allow(clippy::too_many_arguments)]
    fn rkc(&mut self, t: &mut [Float], surface: &Surface<'_>, thermal: Thermal<'_>, dt: Float, dz: Float, bottom: Float, stages: usize) -> Result<()> {
        let rkc = Rkc::new(stages)?;
        let (layers, facets) = self.shape;
        let n = layers * facets;
        let edges = Edges::new(thermal, self.newton, layers, facets, dz, bottom, t)?;
        let [mut z2, mut z1, mut z] = std::mem::take(&mut self.y);
        for v in [&mut z2, &mut z1, &mut z] {
            v.clear();
            v.resize(n, 0.0);
        }
        let result = (|| -> Result<()> {
            // Stage 0: the columns as they are, their boundaries set.
            edges.set(&mut z1, t, surface)?;
            let rate = combine(&mut z, t, &z1, &z1, (1.0, 0.0, rkc.first as Float * dt), thermal, dz, layers, facets)?;
            // The largest rate of a column is under twice the largest sum of
            // a layer's two (Gershgorin), which is what bounds the step.
            let reach = (2.0 * rate * dt) as f64;
            if reach > rkc.beta * (1.0 + 1e-6) {
                let need = Scheme::rkc_stages((reach / 2.0) as Float).map(|s| format!("{s} stages")).unwrap_or_else(|_| "an implicit scheme".into());
                bail!(
                    "unstable: dt times the columns' largest rate is {reach:.4}, past the {:.4} that {stages} RKC stages reach; \
                     this step takes {need}",
                    rkc.beta
                );
            }
            edges.set(&mut z, t, surface)?;
            std::mem::swap(&mut z2, &mut z1);
            std::mem::swap(&mut z1, &mut z);
            for j in 2..=stages {
                let weights = (rkc.mu[j] as Float, rkc.nu[j] as Float, rkc.tilde[j] as Float * dt);
                combine(&mut z, t, &z1, &z2, weights, thermal, dz, layers, facets)?;
                edges.set(&mut z, t, surface)?;
                std::mem::swap(&mut z2, &mut z1);
                std::mem::swap(&mut z1, &mut z);
            }
            add(t, &z1, &mut self.rest, layers, facets);
            Ok(())
        })();
        self.y = [z2, z1, z];
        result
    }

    /// One implicit step: the elimination from the base up, the surface,
    /// and the layers from the top down. Each pass goes along a layer
    /// across the facets, which the compiler vectorises.
    fn implicit(&mut self, t: &mut [Float], surface: &Surface<'_>, thermal: Thermal<'_>, dt: Float, dz: Float, bottom: Float) -> Result<()> {
        let (layers, facets) = self.shape;
        let n = layers * facets;
        // The system for the step's changes `d = T' - T`, at each layer:
        //
        //   -g lo d_{i-1} + (1 + g (lo + hi)) d_i - g hi d_{i+1}
        //       =  c F_i(T) + w (T_i - T_i before)
        //
        // `F` the conduction's rate of change. Backward Euler: g = c = dt.
        // Crank-Nicolson: g = dt / 2, c = dt. BDF2, for a step `r` times
        // the one before: g = c = dt (1 + r) / (1 + 2 r), w = r^2 / (1 + 2 r),
        // which is 2 dt / 3 and 1/3 for equal steps.
        let (mut gamma, mut c, mut w) = (dt, dt, 0.0 as Float);
        if self.scheme == Scheme::CrankNicolson {
            gamma = 0.5 * dt;
        }
        let two_steps = self.scheme == Scheme::Bdf2 && self.history.as_ref().is_some_and(|h| dt / h.dt <= BDF2_LARGEST_RATIO);
        if two_steps {
            let r = dt / self.history.as_ref().expect("checked").dt;
            gamma = dt * (1.0 + r) / (1.0 + 2.0 * r);
            c = gamma;
            w = r * r / (1.0 + 2.0 * r);
        }
        self.p.resize(n, 0.0);
        self.s.resize(n, 0.0);
        if self.scheme == Scheme::Bdf2 {
            self.spare.clear();
            self.spare.extend_from_slice(t);
        }
        let before: &[Float] = match (&self.history, two_steps) {
            (Some(h), true) => &h.t,
            _ => &[],
        };
        let (p, s) = (&mut self.p[..], &mut self.s[..]);
        let row = |i: usize| i * facets..(i + 1) * facets;
        let edges = Edges::new(thermal, self.newton, layers, facets, dz, bottom, t)?;
        let mut rates = Rates::new(thermal, facets, layers, dz, t)?;

        // The base: `T'_last = T'_above + rise`, in changes.
        {
            let (above, last) = (&t[row(layers - 2)], &t[row(layers - 1)]);
            p[row(layers - 1)].fill(1.0);
            for (f, o) in s[row(layers - 1)].iter_mut().enumerate() {
                *o = (above[f] - last[f]) + edges.rise[f];
            }
        }
        let mut rhs = vec![0.0 as Float; facets];
        for i in (1..layers - 1).rev() {
            let (lo, hi) = rates.layer(i, t)?;
            let (above, here, below) = (&t[row(i - 1)], &t[row(i)], &t[row(i + 1)]);
            for f in 0..facets {
                rhs[f] = c * (lo[f] * (above[f] - here[f]) + hi[f] * (below[f] - here[f]));
            }
            if two_steps {
                let before = &before[row(i)];
                for f in 0..facets {
                    rhs[f] += w * (here[f] - before[f]);
                }
            }
            let (p_here, p_below) = p[i * facets..(i + 2) * facets].split_at_mut(facets);
            let (s_here, s_below) = s[i * facets..(i + 2) * facets].split_at_mut(facets);
            for f in 0..facets {
                let (gl, gh) = (gamma * lo[f], gamma * hi[f]);
                let den = 1.0 + gl + gh * (1.0 - p_below[f]);
                p_here[f] = gl / den;
                s_here[f] = (rhs[f] + gh * s_below[f]) / den;
            }
        }
        drop(rates);

        // The surface's change: from what is given, or from the balance
        // with the gradient written from the surface's temperature alone,
        //
        //   -3 T0' + 4 T1' - T2'  =  a T0' + b
        let mut change = vec![0.0 as Float; facets];
        match surface {
            Surface::Temperature(v) => {
                for (f, o) in change.iter_mut().enumerate() {
                    *o = v[f] - t[f];
                }
            }
            Surface::Flux(q) => {
                let (p1, s1) = (&p[row(1)], &s[row(1)]);
                let (p2, s2) = (&p[row(2)], &s[row(2)]);
                let (t1, t2) = (&t[row(1)], &t[row(2)]);
                for f in 0..facets {
                    let a = -3.0 + p1[f] * (4.0 - p2[f]);
                    let b = (-3.0 * t[f] + 4.0 * t1[f] - t2[f]) - a * t[f] + s1[f] * (4.0 - p2[f]) - s2[f];
                    change[f] = edges.balance(f, t[f], q[f], a, b)? - t[f];
                }
            }
        }

        // From the top down, each change added to its temperature.
        for (f, d) in change.iter().enumerate() {
            t[f] += d;
        }
        let rest = &mut self.rest[..];
        for i in 1..layers {
            let (p, s) = (&p[row(i)], &s[row(i)]);
            let (here, rest) = (&mut t[row(i)], &mut rest[row(i)]);
            // The base holds no heat: it follows the layer above it.
            let carry = i < layers - 1;
            for f in 0..facets {
                let d = p[f] * change[f] + s[f];
                change[f] = d;
                let v = here[f];
                if carry {
                    let step = d + rest[f];
                    let next = v + step;
                    rest[f] = step - (next - v);
                    here[f] = next;
                } else {
                    here[f] = v + d;
                }
            }
        }
        if self.scheme == Scheme::Bdf2 {
            let kept = std::mem::take(&mut self.spare);
            if let Some(h) = self.history.take() {
                self.spare = h.t;
            }
            self.history = Some(History { t: kept, dt });
        }
        Ok(())
    }
}

/// The stages' changes added to the temperatures at the end of an RKC
/// step, the layers' with what the sum cannot hold kept in `rest`.
fn add(t: &mut [Float], z: &[Float], rest: &mut [Float], layers: usize, facets: usize) {
    for i in 0..layers {
        let carry = i > 0 && i < layers - 1;
        for f in i * facets..(i + 1) * facets {
            let v = t[f];
            if carry {
                let step = z[f] + rest[f];
                let next = v + step;
                rest[f] = step - (next - v);
                t[f] = next;
            } else {
                t[f] = v + z[f];
            }
        }
    }
}

/// What bounds the columns at a step: at the surface, each facet's `e
/// sigma` and its conductivity over twice the layer's thickness, for the
/// balance; at the base, what the last row sits above the one over it for
/// the heat flow given (K), the conductivity there read at the temperatures
/// of the step's start.
struct Edges {
    se: Vec<Float>,
    kq: Vec<Float>,
    rise: Vec<Float>,
    laws: Laws,
    newton: Newton,
    layers: usize,
    facets: usize,
}

impl Edges {
    fn new(thermal: Thermal<'_>, newton: Newton, layers: usize, facets: usize, dz: Float, q: Float, t: &[Float]) -> Result<Self> {
        let sigma = crate::util::STEFAN_BOLTZMANN;
        let (mut se, mut kq, mut rise) = (vec![0.0 as Float; facets], vec![0.0 as Float; facets], vec![0.0 as Float; facets]);
        let laws = match thermal {
            Thermal::Body(prop) => {
                if !(prop.conductivity > 0.0) {
                    bail!(
                        "the conductivity is {}: compute it from the thermal inertia first, with compute_conductivity_diffusivity()",
                        prop.conductivity
                    );
                }
                se.fill(sigma * prop.emissivity);
                kq.fill(prop.conductivity / (2.0 * dz));
                rise.fill(q * dz / prop.conductivity);
                Laws::default()
            }
            Thermal::Ground(g) => {
                let mut bufs = [RowBuf::new(facets), RowBuf::new(facets)];
                let [b0, b1] = &mut bufs;
                for (f, o) in se.iter_mut().enumerate() {
                    *o = sigma * if g.emissivity.len() == 1 { g.emissivity[0] } else { g.emissivity[f] };
                }
                for (o, &k) in kq.iter_mut().zip(row_of(&g.conductivity, 0, facets, b0)) {
                    *o = k / (2.0 * dz);
                }
                if q != 0.0 {
                    let (above, last) = (&t[(layers - 2) * facets..(layers - 1) * facets], &t[(layers - 1) * facets..]);
                    let (k_above, k_last) = (row_of(&g.conductivity, layers - 2, facets, b0), row_of(&g.conductivity, layers - 1, facets, b1));
                    for f in 0..facets {
                        let k = harmonic(k_above[f] * g.laws.conductivity(above[f]), k_last[f] * g.laws.conductivity(last[f]));
                        if !(k > 0.0) {
                            bail!("a heat flow at the bottom needs a ground that conducts: facet {f}'s last layers do not");
                        }
                        rise[f] = q * dz / k;
                    }
                }
                g.laws
            }
        };
        Ok(Self { se, kq, rise, laws, newton, layers, facets })
    }

    /// Facet `f`'s surface temperature for the flux `q` it absorbs, the
    /// gradient under it `a x + b`, from `start`.
    fn balance(&self, f: usize, start: Float, q: Float, a: Float, b: Float) -> Result<Float> {
        newton_balance(start, q, self.se[f], self.kq[f], &self.laws, a, b, self.newton).map_err(|_| {
            anyhow!(
                "the surface balance of facet {f} did not converge (T0 = {start} K, absorbing {q} W/m2): \
                 the temperatures under it have run away, or the ground's properties cannot be"
            )
        })
    }

    /// The boundaries of a stage of RKC, in its changes `z` from the
    /// temperatures `t` of the step's start: the surface from what bounds
    /// it, the layers under it as the stage has them, and the base from the
    /// layer above it.
    fn set(&self, z: &mut [Float], t: &[Float], surface: &Surface<'_>) -> Result<()> {
        let (layers, facets) = (self.layers, self.facets);
        match surface {
            Surface::Temperature(v) => {
                for f in 0..facets {
                    z[f] = v[f] - t[f];
                }
            }
            Surface::Flux(q) => {
                for f in 0..facets {
                    let (t1, t2) = (t[facets + f] + z[facets + f], t[2 * facets + f] + z[2 * facets + f]);
                    let start = t[f] + z[f];
                    z[f] = self.balance(f, start, q[f], -3.0, 4.0 * t1 - t2)? - t[f];
                }
            }
        }
        let (above, last) = ((layers - 2) * facets, (layers - 1) * facets);
        for f in 0..facets {
            z[last + f] = z[above + f] + (t[above + f] - t[last + f]) + self.rise[f];
        }
        Ok(())
    }
}

/// A ground's rates for the implicit elimination, a layer at a time from
/// the base up: `lo_i = k_{i-1/2} / (rho c_i dz^2)` and `hi_i = k_{i+1/2} /
/// (rho c_i dz^2)`, per facet. One material gives one number for all; a
/// `Ground` gives each layer and facet its own, its laws read at the
/// temperatures as they stand.
struct Rates<'a> {
    facets: usize,
    lo: Vec<Float>,
    hi: Vec<Float>,
    ground: Option<GroundRates<'a>>,
}

struct GroundRates<'a> {
    g: GroundView<'a>,
    inv_dz2: Float,
    /// This layer's conductivity, and that of its boundary below.
    k_here: Vec<Float>,
    face_below: Vec<Float>,
    bufs: [RowBuf; 3],
}

impl<'a> Rates<'a> {
    fn new(thermal: Thermal<'a>, facets: usize, layers: usize, dz: Float, t: &[Float]) -> Result<Self> {
        let zeros = || vec![0.0 as Float; facets];
        match thermal {
            Thermal::Body(prop) => {
                if !(prop.diffusivity > 0.0) {
                    bail!(
                        "the diffusivity is {}: compute it from the thermal inertia first, with compute_conductivity_diffusivity()",
                        prop.diffusivity
                    );
                }
                let r = prop.diffusivity / (dz * dz);
                Ok(Self { facets, lo: vec![r; facets], hi: vec![r; facets], ground: None })
            }
            Thermal::Ground(g) => {
                let mut bufs = [RowBuf::new(facets), RowBuf::new(facets), RowBuf::new(facets)];
                let laws = g.laws;
                let (mut k_here, mut face_below) = (zeros(), zeros());
                {
                    // The two last layers, each at its temperature.
                    let (above, last) = (&t[(layers - 2) * facets..(layers - 1) * facets], &t[(layers - 1) * facets..]);
                    let [b0, b1, _] = &mut bufs;
                    let (k_above, k_last) = (row_of(&g.conductivity, layers - 2, facets, b0), row_of(&g.conductivity, layers - 1, facets, b1));
                    for f in 0..facets {
                        k_here[f] = k_above[f] * laws.conductivity(above[f]);
                        face_below[f] = harmonic(k_here[f], k_last[f] * laws.conductivity(last[f]));
                    }
                }
                Ok(Self { facets, lo: zeros(), hi: zeros(), ground: Some(GroundRates { g, inv_dz2: 1.0 / (dz * dz), k_here, face_below, bufs }) })
            }
        }
    }

    /// Layer `i`'s two rates, the layers asked for from the last but one up
    /// to the first, each once.
    fn layer(&mut self, i: usize, t: &[Float]) -> Result<(&[Float], &[Float])> {
        let facets = self.facets;
        if let Some(GroundRates { g, inv_dz2, k_here, face_below, bufs }) = &mut self.ground {
            let laws = g.laws;
            let [b_k, b_rho, b_c] = bufs;
            let k_up = row_of(&g.conductivity, i - 1, facets, b_k);
            let rho = row_of(&g.density, i, facets, b_rho);
            let c = row_of(&g.heat_capacity, i, facets, b_c);
            let (above, here) = (&t[(i - 1) * facets..i * facets], &t[i * facets..(i + 1) * facets]);
            let (lo, hi) = (&mut self.lo[..facets], &mut self.hi[..facets]);
            let (k_here, face_below) = (&mut k_here[..facets], &mut face_below[..facets]);
            let (mut lowest, mut most) = (0.0 as Float, 0.0 as Float);
            for f in 0..facets {
                let k_above = k_up[f] * laws.conductivity(above[f]);
                let face_above = harmonic(k_above, k_here[f]);
                let inv = *inv_dz2 / (rho[f] * laws.heat_capacity(here[f], c[f]));
                lo[f] = face_above * inv;
                hi[f] = face_below[f] * inv;
                lowest = lowest.min(inv.min(face_above));
                most = most.max(lo[f] + hi[f]);
                k_here[f] = k_above;
                face_below[f] = face_above;
            }
            if !(lowest >= 0.0) || !most.is_finite() {
                bail!(
                    "layer {i} cannot be stepped: a conductivity, a density or a heat capacity of the ground is not positive there \
                     (with laws, at the temperatures the columns have)"
                );
            }
        }
        Ok((&self.lo, &self.hi))
    }
}

/// One stage of RKC, in the stages' changes from the temperatures `t` of
/// the step's start: `out = mu z1 + nu z2 + w F(t + z1)` on the layers, `F`
/// the conduction at that stage's temperatures; the surface and the base as
/// `z1` has them, to be set by their boundaries. Gives the largest sum of a
/// layer's two rates (1/s).
///
/// A difference between two layers is taken as that of their temperatures
/// plus that of their changes, each held to its own digits.
#[allow(clippy::too_many_arguments)]
fn combine(
    out: &mut [Float],
    t: &[Float],
    z1: &[Float],
    z2: &[Float],
    (mu, nu, w): (Float, Float, Float),
    thermal: Thermal<'_>,
    dz: Float,
    layers: usize,
    facets: usize,
) -> Result<Float> {
    let row = |i: usize| i * facets..(i + 1) * facets;
    out[row(0)].copy_from_slice(&z1[row(0)]);
    out[row(layers - 1)].copy_from_slice(&z1[row(layers - 1)]);
    match thermal {
        Thermal::Body(prop) => {
            if !(prop.diffusivity > 0.0) {
                bail!(
                    "the diffusivity is {}: compute it from the thermal inertia first, with compute_conductivity_diffusivity()",
                    prop.diffusivity
                );
            }
            let r = prop.diffusivity / (dz * dz);
            for i in 1..layers - 1 {
                let (ta, th, tb) = (&t[row(i - 1)], &t[row(i)], &t[row(i + 1)]);
                let (za, zh, zb) = (&z1[row(i - 1)], &z1[row(i)], &z1[row(i + 1)]);
                let (old, o) = (&z2[row(i)], &mut out[row(i)]);
                for f in 0..facets {
                    let up = (ta[f] - th[f]) + (za[f] - zh[f]);
                    let down = (tb[f] - th[f]) + (zb[f] - zh[f]);
                    o[f] = mu * zh[f] + nu * old[f] + w * r * (up + down);
                }
            }
            Ok(2.0 * r)
        }
        Thermal::Ground(g) => {
            let laws = g.laws;
            let inv_dz2 = 1.0 / (dz * dz);
            let mut bufs = [RowBuf::new(facets), RowBuf::new(facets), RowBuf::new(facets), RowBuf::new(facets)];
            let zeros = || vec![0.0 as Float; facets];
            // This layer's conductivity and that of its boundary above, the
            // layers gone down.
            let (mut k_here, mut face_above) = (zeros(), zeros());
            {
                let [b0, b1, ..] = &mut bufs;
                let (k0, k1) = (row_of(&g.conductivity, 0, facets, b0), row_of(&g.conductivity, 1, facets, b1));
                for f in 0..facets {
                    k_here[f] = k1[f] * laws.conductivity(t[facets + f] + z1[facets + f]);
                    face_above[f] = harmonic(k0[f] * laws.conductivity(t[f] + z1[f]), k_here[f]);
                }
            }
            let (mut worst, mut lowest) = (0.0 as Float, 0.0 as Float);
            for i in 1..layers - 1 {
                let [b_k, b_rho, b_c, _] = &mut bufs;
                let k_next = row_of(&g.conductivity, i + 1, facets, b_k);
                let rho = row_of(&g.density, i, facets, b_rho);
                let c = row_of(&g.heat_capacity, i, facets, b_c);
                let (ta, th, tb) = (&t[row(i - 1)], &t[row(i)], &t[row(i + 1)]);
                let (za, zh, zb) = (&z1[row(i - 1)], &z1[row(i)], &z1[row(i + 1)]);
                let (old, o) = (&z2[row(i)], &mut out[row(i)]);
                for f in 0..facets {
                    let k_below = k_next[f] * laws.conductivity(tb[f] + zb[f]);
                    let face_below = harmonic(k_here[f], k_below);
                    let inv = inv_dz2 / (rho[f] * laws.heat_capacity(th[f] + zh[f], c[f]));
                    let (lo, hi) = (face_above[f] * inv, face_below * inv);
                    worst = worst.max(lo + hi);
                    lowest = lowest.min(inv.min(face_below));
                    let up = (ta[f] - th[f]) + (za[f] - zh[f]);
                    let down = (tb[f] - th[f]) + (zb[f] - zh[f]);
                    o[f] = mu * zh[f] + nu * old[f] + w * (lo * up + hi * down);
                    k_here[f] = k_below;
                    face_above[f] = face_below;
                }
                if !(lowest >= 0.0) || !worst.is_finite() {
                    bail!(
                        "layer {i} cannot be stepped: a conductivity, a density or a heat capacity of the ground is not positive there \
                         (with laws, at the temperatures the columns have)"
                    );
                }
            }
            Ok(worst)
        }
    }
}

#[cfg(feature = "python")]
pub(crate) mod py {
    use pyo3::prelude::*;

    use super::super::core::py::{Ground, body, per_facet, temperatures, value_error};
    use super::super::core::{GroundView, Newton};
    use super::{Scheme, Surface};
    use crate::Float;

    /// One step of every column at once, by the time scheme of one's
    /// choice: the surface, the layers and the base in one call.
    ///
    /// ```python
    /// stepper = core.Stepper("crank-nicolson")
    /// t = core.columns(layers, facets, t0)
    /// for _ in range(steps):
    ///     stepper.step(t, flux, prop, dt, dz)          # flux: W/m2 absorbed, one a facet
    /// ```
    ///
    /// `scheme` is one of
    ///
    /// - `"explicit"`: `flux_bc`, `heat_conduction` and `bottom_flux`, to
    ///   the bit. First order, stable while `D dt / dz**2 <= 1/2`
    ///   (`stability_maxdt`).
    /// - `"rkc"`: that step in `stages` stages (Runge-Kutta-Chebyshev),
    ///   stable about `stages**2` times further for `stages` times the
    ///   work, still first order. `Stepper.rkc_stages(speedup)` gives the
    ///   fewest stages for a step `speedup` times the explicit one's.
    /// - `"backward-euler"`, `"crank-nicolson"`, `"bdf2"`: implicit, stable
    ///   at any step. The first is first order; the two others second, of
    ///   which BDF2 damps what a step cannot resolve and Crank-Nicolson
    ///   does not. Surface and layers are solved together.
    ///
    /// `prop` is the body's `Properties` or a `Ground`: every scheme takes
    /// layered and graded grounds and those that change with temperature
    /// (`Ground.radiative_conductivity`, `Ground.heat_capacity_polynomial`).
    ///
    /// Newton's method solves the surface balance until a correction is
    /// under `newton_threshold` (K, 0.1 by default), in `newton_iterations`
    /// at most; `newton_iterations=1` is the balance linearised about the
    /// step before, the semi-implicit form.
    ///
    /// One `Stepper` a body: it keeps what its steps could not add to the
    /// temperatures in single precision, and BDF2 the step before.
    #[pyclass(unsendable, module = "kalast._rs.tpm.core")]
    pub struct Stepper {
        inner: super::Stepper,
    }

    impl Stepper {
        #[allow(clippy::too_many_arguments)]
        fn advance(&mut self, t: &Bound<'_, PyAny>, given: &Bound<'_, PyAny>, flux: bool, prop: &Bound<'_, PyAny>, dt: Float, dz: Float, bottom: Float) -> PyResult<()> {
            let mut t = temperatures(t)?;
            let given = per_facet(given, if flux { "flux" } else { "temperature" })?;
            let surface = if flux { Surface::Flux(given.view()) } else { Surface::Temperature(given.view()) };
            if let Ok(g) = prop.extract::<PyRef<'_, Ground>>() {
                let arrays = g.readonly(prop.py())?;
                let view = GroundView { remainder: Some(&g.remainder), ..arrays.view() };
                return self.inner.step(t.as_array_mut(), surface, view, dt, dz, bottom).map_err(value_error);
            }
            let prop = body(prop)?;
            self.inner.step(t.as_array_mut(), surface, &*prop.inner.borrow(), dt, dz, bottom).map_err(value_error)
        }
    }

    #[pymethods]
    impl Stepper {
        #[new]
        #[pyo3(signature = (scheme="explicit", stages=1, newton_threshold=None, newton_iterations=None))]
        fn new(scheme: &str, stages: usize, newton_threshold: Option<Float>, newton_iterations: Option<usize>) -> PyResult<Self> {
            let mut inner = super::Stepper::new(Scheme::named(scheme, stages).map_err(value_error)?);
            let default = Newton::default();
            let newton = Newton { threshold: newton_threshold.unwrap_or(default.threshold), iterations: newton_iterations.unwrap_or(default.iterations) };
            if !(newton.threshold > 0.0 && newton.threshold.is_finite() && newton.iterations >= 1) {
                return Err(pyo3::exceptions::PyValueError::new_err(format!(
                    "Newton's method to {} K in {} iterations: a positive threshold, one iteration or more",
                    newton.threshold, newton.iterations
                )));
            }
            inner.newton = newton;
            Ok(Self { inner })
        }

        /// The scheme's name.
        #[getter]
        fn scheme(&self) -> &'static str {
            self.inner.scheme.name()
        }

        /// The stages of a step: those of `"rkc"`, 1 for any other scheme.
        #[getter]
        fn stages(&self) -> usize {
            match self.inner.scheme {
                Scheme::Rkc { stages } => stages,
                _ => 1,
            }
        }

        #[getter]
        fn newton_threshold(&self) -> Float {
            self.inner.newton.threshold
        }

        #[getter]
        fn newton_iterations(&self) -> usize {
            self.inner.newton.iterations
        }

        /// One step of `dt` (s) of the columns `t`, as `columns` makes
        /// them, their layers `dz` (m) apart, changed in place: each
        /// surface from the `flux` it absorbs (W/m2, one a facet), the
        /// layers by conduction, the base from the heat flow `bottom_flux`
        /// (W/m2) that enters it from below.
        #[pyo3(signature = (t: "numpy.ndarray", flux: "numpy.ndarray", prop: "Properties | Ground", dt, dz, bottom_flux=0.0))]
        fn step(&mut self, t: &Bound<'_, PyAny>, flux: &Bound<'_, PyAny>, prop: &Bound<'_, PyAny>, dt: Float, dz: Float, bottom_flux: Float) -> PyResult<()> {
            self.advance(t, flux, true, prop, dt, dz, bottom_flux)
        }

        /// `step` with each surface's `temperature` (K, one a facet) given
        /// in place of a flux: for a test against a closed form. An
        /// explicit step goes from the surface as it is at the step's
        /// start, an implicit one to the surface as it is at its end.
        #[pyo3(signature = (t: "numpy.ndarray", temperature: "numpy.ndarray", prop: "Properties | Ground", dt, dz, bottom_flux=0.0))]
        fn step_surface(
            &mut self,
            t: &Bound<'_, PyAny>,
            temperature: &Bound<'_, PyAny>,
            prop: &Bound<'_, PyAny>,
            dt: Float,
            dz: Float,
            bottom_flux: Float,
        ) -> PyResult<()> {
            self.advance(t, temperature, false, prop, dt, dz, bottom_flux)
        }

        /// Forget the steps before: BDF2 starts again with a backward-Euler
        /// step. After the temperatures were set from outside.
        fn reset(&mut self) {
            self.inner.reset();
        }

        /// How far `stages` stages of RKC stay stable, in units where the
        /// explicit step's limit is 2: about `2 stages**2`.
        #[staticmethod]
        fn rkc_stability(stages: usize) -> PyResult<Float> {
            Scheme::rkc_stability(stages).map_err(value_error)
        }

        /// The fewest RKC stages for a step `speedup` times the largest the
        /// explicit scheme takes (`stability_maxdt`).
        #[staticmethod]
        fn rkc_stages(speedup: Float) -> PyResult<usize> {
            Scheme::rkc_stages(speedup).map_err(value_error)
        }

        fn __repr__(&self) -> String {
            match self.inner.scheme {
                Scheme::Rkc { stages } => format!("Stepper('rkc', stages={stages})"),
                other => format!("Stepper('{}')", other.name()),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use ndarray::{Array1, Array2, s};

    use super::super::core::{Ground, bottom_flux, columns, flux_bc, heat_conduction, stability_maxdt};
    use super::super::properties::{Properties, skin_depth_1, skin_depth_2pi};
    use super::*;

    const PI: Float = std::f64::consts::PI as Float;
    const SIGMA: Float = crate::util::STEFAN_BOLTZMANN;

    /// Print what a test measures: `KALAST_MEASURE=1 cargo test -- --nocapture`.
    fn measured(what: &str) {
        if std::env::var("KALAST_MEASURE").is_ok() {
            println!("    measured: {what}");
        }
    }

    fn material(ti: Float) -> Properties {
        let mut p = Properties {
            albedo: 0.1,
            emissivity: 0.9,
            density: 2000.0,
            heat_capacity: 600.0,
            thermal_inertia: ti,
            ..Default::default()
        };
        p.compute_conductivity_diffusivity();
        p
    }

    /// The depths of a graded ground's rows: each a half of its width and a
    /// half of the one above's under the row above.
    fn depths_of(widths: &[Float]) -> Vec<Float> {
        let mut z = vec![0.0 as Float];
        for i in 1..widths.len() {
            z.push(z[i - 1] + 0.5 * (widths[i - 1] + widths[i]));
        }
        z
    }

    /// The wave that a surface at `300 + 100 sin(-2 pi t / period)` sends
    /// down a half-space: it decays by `e` and lags by a radian each `ls =
    /// sqrt(D period / pi)`.
    fn wave(z: Float, time: Float, ls: Float, period: Float) -> Float {
        300.0 + 100.0 * (-z / ls).exp() * (z / ls - 2.0 * PI * time / period).sin()
    }

    /// A column of `depths` stepped `steps` times a period for four
    /// periods, its surface given as the wave's: the columns at the end,
    /// and the largest error of a layer against the wave over the last
    /// period.
    fn follow<'a>(scheme: Scheme, thermal: impl Into<Thermal<'a>>, depths: &[Float], dz: Float, ls: Float, period: Float, steps: usize) -> (Array2<Float>, Float) {
        let thermal = thermal.into();
        let mut t = columns(depths.len(), 1, 0.0);
        for (i, &z) in depths.iter().enumerate() {
            t[[i, 0]] = wave(z, 0.0, ls, period);
        }
        let dt = period / steps as Float;
        let mut stepper = Stepper::new(scheme);
        let mut worst = 0.0 as Float;
        for k in 0..4 * steps {
            let time = (k + 1) as Float * dt;
            // An explicit step goes from the surface as it is at its start,
            // an implicit one to the surface as it is at its end.
            let given = if scheme.is_implicit() { time } else { k as Float * dt };
            let top = Array1::from_elem(1, wave(0.0, given, ls, period));
            stepper.step(t.view_mut(), Surface::Temperature(top.view()), thermal, dt, dz, 0.0).unwrap();
            if k >= 3 * steps {
                for (i, &z) in depths.iter().enumerate().skip(1) {
                    worst = worst.max((t[[i, 0]] - wave(z, time, ls, period)).abs());
                }
            }
        }
        (t, worst)
    }

    /// `Explicit` is `core`'s three calls, to the bit: for one material and
    /// for a ground with each facet's own, with a heat flow at the base.
    #[test]
    fn explicit_is_cores_three_calls() {
        let prop = material(200.0);
        let (layers, facets, dz, q) = (14, 4, 2e-3, 0.5);
        let mut ground = Ground::new(&prop, layers, facets).unwrap();
        ground.emissivity = ndarray::array![0.9, 0.8, 0.95, 0.7];
        ground.conductivity.slice_mut(s![..5, ..]).mapv_inplace(|k| 0.4 * k);
        ground.laws = Laws { radiative: 1.5, heat_capacity: Some([200.0, 1.5, 0.0, 0.0, 0.0]) };
        let dt_body = stability_maxdt(prop.diffusivity, dz * dz, 0.4);
        let dt_ground = ground.stability_maxdt_between(dz, 0.4, 100.0, 500.0).unwrap();
        let flux = ndarray::array![0.0, 250.0, 700.0, 1300.0];
        let start = || {
            let mut t = columns(layers, facets, 0.0);
            for ((z, f), v) in t.indexed_iter_mut() {
                *v = 240.0 + 12.0 * f as Float - 2.0 * z as Float;
            }
            t
        };
        let (mut a, mut b) = (start(), start());
        let mut stepper = Stepper::new(Scheme::Explicit);
        for _ in 0..40 {
            stepper.step(a.view_mut(), Surface::Flux(flux.view()), &prop, dt_body, dz, q).unwrap();
            flux_bc(b.view_mut(), flux.view(), &prop, dz).unwrap();
            heat_conduction(b.view_mut(), &prop, dt_body, dz).unwrap();
            bottom_flux(b.view_mut(), q, &prop, dz).unwrap();
        }
        assert_eq!(a, b);
        assert!(a != start());

        let twin = ground.clone();
        let (mut a, mut b) = (start(), start());
        for _ in 0..40 {
            stepper.step(a.view_mut(), Surface::Flux(flux.view()), &ground, dt_ground, dz, q).unwrap();
            flux_bc(b.view_mut(), flux.view(), &twin, dz).unwrap();
            heat_conduction(b.view_mut(), &twin, dt_ground, dz).unwrap();
            bottom_flux(b.view_mut(), q, &twin, dz).unwrap();
        }
        assert_eq!(a, b);
    }

    /// Every scheme follows the wave of a surface that swings by 100 K, on
    /// equal layers ten a skin depth and on a graded column of 18. The
    /// budgets are what each measures with a third over; the second-order
    /// schemes are at what the layers themselves allow, 0.044 K and 0.15 K.
    #[test]
    fn every_scheme_follows_the_wave() {
        let prop = material(320.0);
        let period: Float = 8136.0;
        let ls = skin_depth_1(prop.diffusivity, period);
        let dz = ls / 10.0;
        let equal: Vec<Float> = (0..=80).map(|i| i as Float * dz).collect();
        let limit = stability_maxdt(prop.diffusivity, dz * dz, 0.5);
        // steps a period, scheme, budget (K)
        let cases = [
            (200, Scheme::BackwardEuler, 0.60),
            (200, Scheme::CrankNicolson, 0.058),
            (200, Scheme::Bdf2, 0.060),
            (800, Scheme::Explicit, 0.080),
            (400, Scheme::Rkc { stages: 2 }, 0.32),
        ];
        for (steps, scheme, budget) in cases {
            let (_, e) = follow(scheme, &prop, &equal, dz, ls, period, steps);
            measured(&format!("equal layers, {} at {steps} steps a period ({:.1} x the explicit limit): {e:.4} K", scheme.name(), period / steps as Float / limit));
            assert!(e < budget, "{} at {steps} steps: {e} K, over {budget}", scheme.name());
        }
        let (ground, widths) = Ground::graded(&prop, 1, dz, 8.0 * ls, 1.2).unwrap();
        let graded = depths_of(&widths);
        assert_eq!(widths.len(), 18);
        for (scheme, budget) in [(Scheme::BackwardEuler, 0.50), (Scheme::CrankNicolson, 0.20), (Scheme::Bdf2, 0.20)] {
            let (_, e) = follow(scheme, &ground, &graded, dz, ls, period, 200);
            measured(&format!("graded, {} at 200 steps a period: {e:.4} K", scheme.name()));
            assert!(e < budget, "{} on the graded column: {e} K, over {budget}", scheme.name());
        }
    }

    /// Halving the step halves backward Euler's error and quarters
    /// Crank-Nicolson's and BDF2's: against the same columns stepped 3,200
    /// times a period, which leaves the error of the layers out.
    #[test]
    fn the_implicit_schemes_converge_at_their_order() {
        let prop = material(320.0);
        let period: Float = 8136.0;
        let ls = skin_depth_1(prop.diffusivity, period);
        let dz = ls / 10.0;
        let depths: Vec<Float> = (0..=80).map(|i| i as Float * dz).collect();
        let (reference, _) = follow(Scheme::CrankNicolson, &prop, &depths, dz, ls, period, 3200);
        for (scheme, want) in [(Scheme::BackwardEuler, 1.0), (Scheme::CrankNicolson, 2.0), (Scheme::Bdf2, 2.0)] {
            let errors: Vec<Float> = [25usize, 50, 100]
                .iter()
                .map(|&steps| {
                    let (t, _) = follow(scheme, &prop, &depths, dz, ls, period, steps);
                    (&t - &reference).mapv(Float::abs).fold(0.0, |m: Float, v| m.max(*v))
                })
                .collect();
            let orders: Vec<Float> = errors.windows(2).map(|e| (e[0] / e[1]).log2()).collect();
            let mean = orders.iter().sum::<Float>() / orders.len() as Float;
            measured(&format!("{}: errors {errors:?} K at 25, 50, 100 steps a period, orders {orders:?}", scheme.name()));
            assert!((mean - want).abs() < 0.3, "{} converges at order {mean}, not {want}: {errors:?}", scheme.name());
        }
    }

    /// A flux that does not change settles each column at the temperature
    /// that radiates it, by every scheme.
    #[test]
    fn a_constant_flux_settles_at_its_balance() {
        let prop = material(200.0);
        let (layers, dz) = (12, 2e-3);
        let flux = ndarray::array![300.0, 900.0];
        let limit = stability_maxdt(prop.diffusivity, dz * dz, 0.5);
        let stages = Scheme::rkc_stages(5.0).unwrap();
        // scheme, step (s), steps
        let cases = [
            (Scheme::Explicit, 0.9 * limit, 3000),
            (Scheme::Rkc { stages }, 5.0 * limit, 600),
            (Scheme::BackwardEuler, 50.0 * limit, 200),
            (Scheme::CrankNicolson, 50.0 * limit, 200),
            (Scheme::Bdf2, 50.0 * limit, 200),
        ];
        for (scheme, dt, steps) in cases {
            let mut t = columns(layers, 2, 250.0);
            let mut stepper = Stepper::new(scheme);
            for _ in 0..steps {
                stepper.step(t.view_mut(), Surface::Flux(flux.view()), &prop, dt, dz, 0.0).unwrap();
            }
            for f in 0..2 {
                let want = (flux[f] / (SIGMA * prop.emissivity)).powf(0.25);
                let off = t.column(f).iter().map(|v| (v - want).abs()).fold(0.0, Float::max);
                measured(&format!("{} settles facet {f} within {off:.5} K of {want:.3} K", scheme.name()));
                assert!(off < 0.02, "{}: facet {f} is {off} K off {want} K", scheme.name());
            }
        }
    }

    /// The flux a surface absorbs at step `step` of `steps` a day, noon at
    /// step 0, with or without an eclipse of the Sun from 0.05 to 0.11 of
    /// the day. In whole steps, so that runs of different steps cut the
    /// light at the same instant.
    fn sunlight(step: usize, steps: usize, eclipse: bool) -> Float {
        let of_day = step % steps;
        if eclipse && (steps / 20..steps * 11 / 100).contains(&of_day) {
            return 0.0;
        }
        1361.0 * 0.9 * (2.0 * PI * of_day as Float / steps as Float).cos().max(0.0) / (1.2 * 1.2)
    }

    /// Six days of a spinning column, `steps` a day: the surface over the
    /// last, 200 values a day. A surface belongs to the time of the flux it
    /// balances: the start of an explicit step, the end of an implicit one.
    fn day(scheme: Scheme, newton: Option<Newton>, steps: usize, eclipse: bool) -> Vec<Float> {
        let prop = material(320.0);
        let period: Float = 8136.0;
        let ls = skin_depth_1(prop.diffusivity, period);
        let dz = ls / 8.0;
        let layers = (skin_depth_2pi(prop.diffusivity, period) / dz).ceil() as usize + 1;
        let mut t = columns(layers, 1, 230.0);
        let dt = period / steps as Float;
        let mut stepper = Stepper::new(scheme);
        if let Some(n) = newton {
            stepper.newton = n;
        }
        let every = steps / 200;
        let mut out = Vec::new();
        for k in 0..6 * steps {
            let given = if scheme.is_implicit() { k + 1 } else { k };
            let flux = Array1::from_elem(1, sunlight(given, steps, eclipse));
            stepper.step(t.view_mut(), Surface::Flux(flux.view()), &prop, dt, dz, 0.0).unwrap();
            if given >= 5 * steps && given < 6 * steps && given % every == 0 {
                out.push(t[[0, 0]]);
            }
        }
        out
    }

    fn apart(a: &[Float], b: &[Float]) -> Float {
        assert_eq!(a.len(), b.len());
        a.iter().zip(b).map(|(x, y)| (x - y).abs()).fold(0.0, Float::max)
    }

    /// A day's surface temperatures are the same by every scheme: against
    /// the explicit one at 3,200 steps a day, the others at a step that the
    /// explicit one cannot take.
    #[test]
    fn a_day_is_the_same_by_every_scheme() {
        let reference = day(Scheme::Explicit, None, 3200, false);
        assert_eq!(reference.len(), 200);
        let swing = reference.iter().copied().fold(0.0, Float::max) - reference.iter().copied().fold(Float::INFINITY, Float::min);
        measured(&format!("the day's surface swings by {swing:.1} K"));
        assert!(swing > 60.0, "a day that swings by {swing} K");
        let stages = Scheme::rkc_stages(2.2).unwrap();
        let cases = [
            (Scheme::BackwardEuler, 200, 0.60),
            (Scheme::CrankNicolson, 200, 0.07),
            (Scheme::Bdf2, 200, 0.07),
            (Scheme::Rkc { stages }, 200, 1.6),
        ];
        for (scheme, steps, budget) in cases {
            let e = apart(&day(scheme, None, steps, false), &reference);
            measured(&format!("a day by {} at {steps} steps: {e:.4} K from the explicit one at 3,200", scheme.name()));
            assert!(e < budget, "{} at {steps} steps a day is {e} K from the reference, over {budget}", scheme.name());
        }
    }

    /// The Sun eclipsed for a twentieth of the day, its light cut at once:
    /// the implicit schemes follow it, and none rings -- through the
    /// eclipse the surface only cools, step after step, Crank-Nicolson's
    /// too. They are furthest from the reference at the instant of the cut,
    /// where the surface falls by what one step lets the cold reach, and
    /// closer by more than two with steps a quarter as long.
    #[test]
    fn an_eclipse_is_followed_without_ringing() {
        let reference = day(Scheme::Explicit, None, 3200, true);
        for (scheme, budget) in [(Scheme::BackwardEuler, 6.0), (Scheme::CrankNicolson, 3.5), (Scheme::Bdf2, 4.4)] {
            let got = day(scheme, None, 400, true);
            let e = apart(&got, &reference);
            let finer = apart(&day(scheme, None, 1600, true), &reference);
            // Samples 10 to 21 are within the eclipse, 0.05 to 0.11 of the day.
            let rises = got[10..22].windows(2).map(|w| w[1] - w[0]).fold(Float::NEG_INFINITY, Float::max);
            let at = got.iter().zip(&reference).map(|(a, b)| (a - b).abs()).enumerate().fold((0, 0.0), |m, (i, e)| if e > m.1 { (i, e) } else { m }).0;
            measured(&format!(
                "an eclipse by {}: {e:.4} K from the reference at 400 steps a day (at sample {at} of 200), {finer:.4} K at 1,600; largest rise within it {rises:.5} K",
                scheme.name()
            ));
            assert!(e < budget && finer < 0.5 * e, "{}: {e} K from the reference through an eclipse, {finer} K with finer steps", scheme.name());
            assert!(rises < 0.0, "{} warms by {rises} K within the eclipse", scheme.name());
        }
        let drop = reference[9] - reference[21];
        measured(&format!("the eclipse cools the surface by {drop:.1} K"));
        assert!(drop > 20.0, "an eclipse that cools the surface by {drop} K");
    }

    /// One iteration of Newton's method is the balance linearised about the
    /// step before, the semi-implicit form: over a day it stays close to
    /// the balance solved in full.
    #[test]
    fn one_iteration_is_the_linearised_balance() {
        let full = day(Scheme::Bdf2, Some(Newton { threshold: 1e-4, iterations: 50 }), 400, false);
        let once = day(Scheme::Bdf2, Some(Newton { threshold: 1e-4, iterations: 1 }), 400, false);
        let e = apart(&full, &once);
        measured(&format!("one Newton iteration a step, 400 steps a day: {e:.4} K from the balance solved in full"));
        assert!(e > 0.0 && e < 1.0, "{e} K");
    }

    /// The temperature that the heat flow `q` reaches at depth `z` under a
    /// surface at `ts`, through a ground of `k = kc (1 + chi (T / 350)^3)`:
    /// `kc (T + chi T^4 / (4 350^3))` grows by `q z`.
    fn kirchhoff(ts: f64, q: f64, z: f64, kc: f64, chi: f64) -> f64 {
        let theta = |t: f64| t + chi * t.powi(4) / (4.0 * 350f64.powi(3));
        let want = theta(ts) + q * z / kc;
        let (mut lo, mut hi) = (ts, 2000.0);
        for _ in 0..200 {
            let mid = 0.5 * (lo + hi);
            if theta(mid) < want { lo = mid } else { hi = mid }
        }
        0.5 * (lo + hi)
    }

    /// A heat flow through a ground whose conductivity grows as `T^3`: the
    /// column settles on the closed form, closer by four with layers half
    /// as thick, and the explicit schemes hold what the implicit one found.
    #[test]
    fn a_heat_flow_through_a_ground_that_conducts_as_t_cubed() {
        let (kc, chi, q, depth): (Float, Float, Float, Float) = (0.005, 2.7, 12.0, 0.2);
        let tops = ndarray::array![100.0, 250.0];
        let settle = |layers: usize| -> (Array2<Float>, Ground, Float, Float) {
            let dz = depth / (layers - 1) as Float;
            let mut ground = Ground::new(&material(200.0), layers, 2).unwrap();
            ground.conductivity.fill(kc);
            ground.laws = Laws { radiative: chi, heat_capacity: None };
            let mut t = columns(layers, 2, 200.0);
            let mut stepper = Stepper::new(Scheme::BackwardEuler);
            for _ in 0..400 {
                stepper.step(t.view_mut(), Surface::Temperature(tops.view()), &ground, 1e7, dz, q).unwrap();
            }
            let mut worst = 0.0 as Float;
            for f in 0..2 {
                for i in 1..layers {
                    let want = kirchhoff(tops[f] as f64, q as f64, (i as Float * dz) as f64, kc as f64, chi as f64) as Float;
                    worst = worst.max((t[[i, f]] - want).abs());
                }
            }
            (t, ground, dz, worst)
        };
        let (t, ground, dz, coarse) = settle(41);
        let (_, _, _, fine) = settle(81);
        measured(&format!("a heat flow through k(T): {coarse:.4} K off the closed form with 41 layers, {fine:.4} K with 81; the base at {:.2} K and {:.2} K", t[[40, 0]], t[[40, 1]]));
        assert!(coarse < 0.2 && fine < coarse / 3.0, "{coarse} K with 41 layers, {fine} K with 81");
        assert!(t[[40, 1]] > 380.0, "a base at {} K", t[[40, 1]]);

        let dt = ground.stability_maxdt_between(dz, 0.4, 90.0, 450.0).unwrap();
        for scheme in [Scheme::Explicit, Scheme::Rkc { stages: 3 }, Scheme::CrankNicolson] {
            let mut held = t.clone();
            let mut stepper = Stepper::new(scheme);
            for _ in 0..20 {
                stepper.step(held.view_mut(), Surface::Temperature(tops.view()), &ground, dt, dz, q).unwrap();
            }
            let moved = (&held - &t).mapv(Float::abs).fold(0.0, |m: Float, v| m.max(*v));
            measured(&format!("{} moves the settled columns by {moved:.5} K in 20 steps", scheme.name()));
            assert!(moved < 5e-3, "{} moves the settled columns by {moved} K", scheme.name());
        }
    }

    /// A column that radiates nothing gains the heat it absorbs, with a
    /// heat capacity that grows with temperature: its layers' enthalpy, and
    /// the half layer at the surface, against the flux times the time.
    #[test]
    fn a_column_gains_the_heat_it_absorbs() {
        let (layers, dz, q, span): (usize, Float, Float, Float) = (30, 2e-3, 60.0, 100_000.0);
        let prop = Properties { emissivity: 0.0, ..material(200.0) };
        let mut ground = Ground::new(&prop, layers, 1).unwrap();
        ground.laws = Laws::MOON;
        let poly = Laws::MOON.heat_capacity.unwrap();
        let enthalpy = |t: Float| -> f64 {
            let t = t as f64;
            (0..5).map(|n| poly[n] as f64 * t.powi(n as i32 + 1) / (n as f64 + 1.0)).sum()
        };
        let limit = ground.stability_maxdt_between(dz, 0.5, 150.0, 450.0).unwrap();
        let flux = ndarray::array![q];
        let cases = [
            (Scheme::Explicit, 0.8 * limit),
            (Scheme::Rkc { stages: 3 }, 3.0 * limit),
            (Scheme::BackwardEuler, 6.0 * limit),
            (Scheme::CrankNicolson, 6.0 * limit),
            (Scheme::Bdf2, 6.0 * limit),
        ];
        for (scheme, dt) in cases {
            let steps = (span / dt).round() as usize;
            let mut t = columns(layers, 1, 200.0);
            let mut stepper = Stepper::new(scheme);
            for _ in 0..steps {
                stepper.step(t.view_mut(), Surface::Flux(flux.view()), &ground, dt, dz, 0.0).unwrap();
            }
            let rho = prop.density as f64;
            let mut gained = 0.5 * (enthalpy(t[[0, 0]]) - enthalpy(200.0));
            for i in 1..layers - 1 {
                gained += enthalpy(t[[i, 0]]) - enthalpy(200.0);
            }
            let gained = gained * rho * dz as f64;
            let absorbed = q as f64 * dt as f64 * steps as f64;
            let gap = gained / absorbed - 1.0;
            measured(&format!("{} at {:.1} x the explicit limit: gains {:.2} % {} than it absorbs; the surface at {:.1} K, the base at {:.1} K", scheme.name(), dt / limit, 100.0 * gap.abs(), if gap > 0.0 { "more" } else { "less" }, t[[0, 0]], t[[layers - 1, 0]]));
            assert!(gap.abs() < 0.02, "{}: gained {gained} J/m2 of {absorbed}", scheme.name());
            assert!(t[[0, 0]] > 285.0 && t[[layers - 1, 0]] > 265.0, "columns heated to {} K at the surface, {} K at the base", t[[0, 0]], t[[layers - 1, 0]]);
        }
    }

    /// Laws that change nothing change nothing: no radiative part and a
    /// polynomial that is the ground's own constant give the steps of the
    /// ground without laws -- to the bit for the explicit scheme.
    #[test]
    fn laws_that_change_nothing_change_nothing() {
        let prop = material(200.0);
        let (layers, facets, dz) = (16, 3, 2e-3);
        let plain = Ground::new(&prop, layers, facets).unwrap();
        let mut lawful = plain.clone();
        lawful.laws = Laws { radiative: 0.0, heat_capacity: Some([prop.heat_capacity, 0.0, 0.0, 0.0, 0.0]) };
        let limit = plain.stability_maxdt(dz, 0.5).unwrap();
        assert_eq!(lawful.stability_maxdt_between(dz, 0.5, 50.0, 500.0).unwrap(), limit);
        let flux = ndarray::array![100.0, 500.0, 1100.0];
        for (scheme, dt) in [(Scheme::Explicit, 0.8 * limit), (Scheme::Rkc { stages: 2 }, 2.0 * limit), (Scheme::Bdf2, 8.0 * limit)] {
            let (mut a, mut b) = (columns(layers, facets, 220.0), columns(layers, facets, 220.0));
            let (mut sa, mut sb) = (Stepper::new(scheme), Stepper::new(scheme));
            for _ in 0..50 {
                sa.step(a.view_mut(), Surface::Flux(flux.view()), &plain, dt, dz, 0.0).unwrap();
                sb.step(b.view_mut(), Surface::Flux(flux.view()), &lawful, dt, dz, 0.0).unwrap();
            }
            assert_eq!(a, b, "{}", scheme.name());
            assert!(a[[0, 2]] > 300.0);
        }
    }

    /// A view of every other facet of an array steps as those facets in an
    /// array of their own.
    #[test]
    fn a_view_with_gaps_steps_as_one_without() {
        let prop = material(200.0);
        let (layers, dz) = (10, 2e-3);
        let mut wide = columns(layers, 6, 0.0);
        for ((z, f), v) in wide.indexed_iter_mut() {
            *v = 200.0 + 10.0 * f as Float + z as Float;
        }
        let mut own = wide.slice(s![.., ..;2]).to_owned();
        let flux = ndarray::array![200.0, 600.0, 1000.0];
        let dt = 10.0 * stability_maxdt(prop.diffusivity, dz * dz, 0.5);
        let (mut a, mut b) = (Stepper::new(Scheme::CrankNicolson), Stepper::new(Scheme::CrankNicolson));
        for _ in 0..10 {
            a.step(wide.slice_mut(s![.., ..;2]), Surface::Flux(flux.view()), &prop, dt, dz, 0.0).unwrap();
            b.step(own.view_mut(), Surface::Flux(flux.view()), &prop, dt, dz, 0.0).unwrap();
        }
        assert_eq!(wide.slice(s![.., ..;2]), own);
        // The facets between were not touched.
        assert_eq!(wide[[3, 1]], 200.0 + 10.0 + 3.0);
    }

    /// BDF2 takes steps of unequal length: alternating between two, it ends
    /// where Crank-Nicolson does, and its history is forgotten on `reset`.
    #[test]
    fn bdf2_takes_steps_of_unequal_length() {
        let prop = material(320.0);
        let period: Float = 8136.0;
        let ls = skin_depth_1(prop.diffusivity, period);
        let dz = ls / 10.0;
        let depths: Vec<Float> = (0..=80).map(|i| i as Float * dz).collect();
        let run = |scheme: Scheme, reset_at: Option<usize>| {
            let mut t = columns(depths.len(), 1, 0.0);
            for (i, &z) in depths.iter().enumerate() {
                t[[i, 0]] = wave(z, 0.0, ls, period);
            }
            let mut stepper = Stepper::new(scheme);
            let mut time = 0.0 as Float;
            for k in 0..600 {
                // 40 s and 80 s in turn: 120 s a pair, 300 pairs.
                let dt = if k % 2 == 0 { 40.0 } else { 80.0 };
                time += dt;
                if reset_at == Some(k) {
                    stepper.reset();
                }
                let top = Array1::from_elem(1, wave(0.0, time, ls, period));
                stepper.step(t.view_mut(), Surface::Temperature(top.view()), &prop, dt, dz, 0.0).unwrap();
            }
            t
        };
        let (cn, bdf2, restarted) = (run(Scheme::CrankNicolson, None), run(Scheme::Bdf2, None), run(Scheme::Bdf2, Some(300)));
        let e = (&cn - &bdf2).mapv(Float::abs).fold(0.0, |m: Float, v| m.max(*v));
        let r = (&restarted - &bdf2).mapv(Float::abs).fold(0.0, |m: Float, v| m.max(*v));
        measured(&format!("BDF2 on steps of 40 s and 80 s in turn: {e:.4} K from Crank-Nicolson; a reset half way moves it by {r:.5} K"));
        assert!(e < 0.05, "{e} K from Crank-Nicolson");
        assert!(r > 0.0 && r < 0.01, "a reset moves it by {r} K");
    }

    /// RKC reaches as far as its polynomial: one stage is forward Euler,
    /// ten reach near two hundred times its two, and a step past what the
    /// stages reach is refused with the stages it takes.
    #[test]
    fn rkc_reaches_as_far_as_its_polynomial() {
        assert!((Scheme::rkc_stability(1).unwrap() - 2.0).abs() < 1e-6);
        let ten = Scheme::rkc_stability(10).unwrap();
        assert!(ten > 180.0 && ten < 200.0, "{ten}");
        for stages in [2usize, 3, 5, 10, 40] {
            let reach = Scheme::rkc_stability(stages).unwrap() / 2.0;
            assert_eq!(Scheme::rkc_stages(0.999 * reach).unwrap(), stages);
            assert_eq!(Scheme::rkc_stages(1.001 * reach).unwrap(), stages + 1);
        }
        assert!(Scheme::rkc_stability(0).is_err() && Scheme::rkc_stages(0.0).is_err() && Scheme::rkc_stages(1e9).is_err());

        let prop = material(200.0);
        let dz = 2e-3;
        let limit = stability_maxdt(prop.diffusivity, dz * dz, 0.5);
        let flux = ndarray::array![400.0];
        let mut t = columns(12, 1, 250.0);
        let before = t.clone();
        let mut stepper = Stepper::new(Scheme::Rkc { stages: 3 });
        let reach = Scheme::rkc_stability(3).unwrap() / 2.0;
        stepper.step(t.view_mut(), Surface::Flux(flux.view()), &prop, 0.99 * reach * limit, dz, 0.0).unwrap();
        let mut t = before.clone();
        let err = stepper.step(t.view_mut(), Surface::Flux(flux.view()), &prop, 1.5 * reach * limit, dz, 0.0).unwrap_err();
        assert!(err.to_string().contains("3 RKC stages") && err.to_string().contains("4 stages"), "{err}");
        assert_eq!(t, before, "an error changes nothing");
    }

    #[test]
    fn a_step_refuses_what_cannot_be() {
        let prop = material(200.0);
        let (dz, dt) = (2e-3, 100.0);
        let flux = ndarray::array![400.0, 500.0];
        for scheme in [Scheme::Explicit, Scheme::Rkc { stages: 2 }, Scheme::Bdf2] {
            let mut stepper = Stepper::new(scheme);
            let mut t = columns(8, 2, 250.0);
            let refused = |e: anyhow::Result<()>, word: &str| {
                let e = e.unwrap_err().to_string();
                assert!(e.contains(word), "{}: {e}", scheme.name());
            };
            refused(stepper.step(columns(2, 2, 250.0).view_mut(), Surface::Flux(flux.view()), &prop, dt, dz, 0.0), "3 layers");
            refused(stepper.step(t.view_mut(), Surface::Flux(ndarray::array![400.0].view()), &prop, dt, dz, 0.0), "1 absorbed fluxes for 2 facets");
            refused(stepper.step(t.view_mut(), Surface::Flux(ndarray::array![400.0, -1.0].view()), &prop, dt, dz, 0.0), "facet 1");
            refused(stepper.step(t.view_mut(), Surface::Temperature(ndarray::array![250.0, 0.0].view()), &prop, dt, dz, 0.0), "facet 1");
            refused(stepper.step(t.view_mut(), Surface::Flux(flux.view()), &prop, -1.0, dz, 0.0), "positive");
            refused(stepper.step(t.view_mut(), Surface::Flux(flux.view()), &prop, dt, dz, Float::NAN), "finite");
            let raw = Properties { diffusivity: 0.0, conductivity: 0.0, ..prop };
            refused(stepper.step(t.view_mut(), Surface::Flux(flux.view()), &raw, dt, dz, 0.0), "compute");
            assert_eq!(t, columns(8, 2, 250.0), "{}: an error changes nothing", scheme.name());
        }
        assert!(Scheme::named("leapfrog", 0).is_err() && Scheme::named("rkc", 0).is_err());
        for name in Scheme::NAMES {
            assert_eq!(Scheme::named(name, 3).unwrap().name(), name);
        }
    }

    /// A heat flow at the base is held by the implicit schemes as `core`
    /// holds it: the columns of `T0 + q z / k` stay where they are.
    #[test]
    fn a_heat_flow_at_the_base_is_held() {
        let prop = material(200.0);
        let (layers, dz, q) = (12, 2e-3, 0.5);
        let slope = q * dz / prop.conductivity;
        let mut start = columns(layers, 2, 0.0);
        for ((z, f), v) in start.indexed_iter_mut() {
            *v = 250.0 + 5.0 * f as Float + slope * z as Float;
        }
        let tops = start.row(0).to_owned();
        let dt = 20.0 * stability_maxdt(prop.diffusivity, dz * dz, 0.5);
        for scheme in [Scheme::BackwardEuler, Scheme::CrankNicolson, Scheme::Bdf2, Scheme::Rkc { stages: 7 }] {
            let mut t = start.clone();
            let mut stepper = Stepper::new(scheme);
            for _ in 0..100 {
                stepper.step(t.view_mut(), Surface::Temperature(tops.view()), &prop, dt, dz, q).unwrap();
            }
            let moved = (&t - &start).mapv(Float::abs).fold(0.0, |m: Float, v| m.max(*v));
            measured(&format!("{} moves the columns of a heat flow by {moved:.6} K in 100 steps", scheme.name()));
            assert!(moved < 1e-3, "{} moved the columns of a heat flow by {moved} K", scheme.name());
        }
    }
}
