// Didymos and Dimorphos, each facet's surface temperature from kalast's
// thermophysical model: Didymos turning, Dimorphos along its orbit facing it,
// the heat conducted down a column under every facet.
//
// Written with callbacks and `start()` rather than a loop of `step()`s, which
// is what lets it run in a browser too, at kalast.greghy.com, as the page was
// built with it.
//
// DART's shape models, Didymos v003 (Palmer et al.) and Dimorphos v004 (Daly,
// Ernst et al.), made by SPC and decimated to 10,000 facets each, in km.

use std::sync::Mutex;

use kalast::app::App;
use kalast::app::config::{ColorMode, Hud, builtin_colormap};
use kalast::app::simulation::Simulation;
use kalast::locked::Locked;
use kalast::ndarray::{Array2, ArrayView1};
use kalast::tpm::core;
use kalast::tpm::properties::{self, Properties};
use kalast::util::AU_KM;
use kalast::{Float, Mat4, Quat, Vec3};

// The surface: thermal inertia (J m-2 K-1 s-1/2), Bond albedo, emissivity.
const INERTIA: Float = 320.0;
const ALBEDO: Float = 0.07;
const EMISSIVITY: Float = 0.9;
// The distance to the Sun (au), and hours of the model a second.
const DAU: Float = 1.0;
const SPEED: Float = 0.5;

// Didymos's spin and Dimorphos's orbit after DART, which is its day too (s);
// their centres apart (km).
const SPIN: Float = 2.2600 * 3600.0;
const ORBIT: Float = 11.3676 * 3600.0;
const SEPARATION: Float = 1.19;

/// One body's ground: the temperature of each layer under each facet (K),
/// the layers a skin depth over 8 thick and 2 pi skin depths deep for the
/// body's day, as `examples/sphere/tpm.py` makes them.
struct Ground {
    prop: Properties,
    t: Array2<Float>,
    dz: Float,
    dt: Float,
}

impl Ground {
    /// Each facet's column starting at its latitude's effective temperature.
    fn new(sim: &Simulation, body: usize, period: Float) -> Self {
        let mut prop = Properties::new(ALBEDO, EMISSIVITY, 2700.0, 600.0, INERTIA, 0.0, 0.0);
        prop.compute_conductivity_diffusivity();
        let dz = properties::skin_depth_1(prop.diffusivity, period) / 8.0;
        let layers = (properties::skin_depth_2pi(prop.diffusivity, period) / dz).round() as usize + 1;
        let start: Vec<Float> = sim
            .facet_mean_incidence(body, Vec3::Z)
            .unwrap()
            .iter()
            .map(|&cosi| core::effective_temperature(DAU, cosi, ALBEDO, EMISSIVITY))
            .collect();
        let t = Array2::from_shape_fn((layers, start.len()), |(_, i)| start[i]);
        let dt = core::stability_maxdt(prop.diffusivity, dz * dz, 0.5);
        Self { prop, t, dz, dt }
    }

    fn step(&mut self, sim: &Simulation, body: usize, dt: Float) {
        let cosi = sim.facet_incidence(body).unwrap();
        core::solar_bc(self.t.view_mut(), DAU, ArrayView1::from(&cosi), &self.prop, self.dz).unwrap();
        core::bottom_adiabatic(self.t.view_mut());
        core::heat_conduction(self.t.view_mut(), &self.prop, dt, self.dz).unwrap();
    }
}

/// Didymos turned and Dimorphos along its orbit at `t` (s), its long axis
/// to Didymos.
fn pose(sim: &mut Simulation, t: Float) {
    let spin = std::f64::consts::TAU as Float * t / SPIN;
    let orbit = std::f64::consts::TAU as Float * t / ORBIT;
    sim.bodies[0].mat = Mat4::from_rotation_z(spin);
    sim.bodies[1].mat =
        Mat4::from_rotation_translation(Quat::from_rotation_z(orbit), Vec3::new(orbit.cos(), orbit.sin(), 0.0) * SEPARATION);
}

fn main() {
    let mut app = App::new();
    {
        let config = app.sim_config();
        let mut c = config.borrow_mut();
        c.shading.color_mode = ColorMode::Data;
        c.data.colormap = builtin_colormap("inferno").unwrap();
        c.data.value_min = Some(150.0);
        c.data.value_max = Some(400.0);
        c.colorbar.enabled = true;
        c.colorbar.label = "Surface temperature (K)".to_string();
        c.colorbar.ticks = 6;
        // The facets' colours alone: 10,000 edges a body darken them.
        c.wireframe.mode = 0;
    }

    let model = {
        let mut sim = app.simulation.borrow_mut();
        sim.load_mesh("res/didymos_v003_10k.obj", Mat4::IDENTITY, false);
        sim.load_mesh("res/dimorphos_v004_10k.obj", Mat4::IDENTITY, false);
        sim.sun.pos = Vec3::X * DAU * AU_KM;
        sim.camera.pos = Vec3::new(3.6, -4.8, 2.1);
        sim.camera.look_anchor();
        let mut hud = Hud::new("");
        hud.size = 15.0;
        sim.huds = vec![Locked::new(hud)];
        pose(&mut sim, 0.0);
        // The model's time (s) and the two grounds.
        Mutex::new((0.0, [Ground::new(&sim, 0, SPIN), Ground::new(&sim, 1, ORBIT)]))
    };

    app.set_before_render(move |sim: &mut Simulation, dt: Float| {
        let (t, grounds) = &mut *model.lock().unwrap();

        // The frame's share of the model's time, in steps no longer than the
        // thinner column allows, and no more than 200 of them a frame.
        let span = SPEED * 3600.0 * dt.clamp(0.0, 0.1);
        let steps = (span / grounds[0].dt.min(grounds[1].dt)).ceil().clamp(1.0, 200.0);
        for _ in 0..steps as usize {
            *t += span / steps;
            pose(sim, *t);
            for (i, ground) in grounds.iter_mut().enumerate() {
                ground.step(sim, i, span / steps);
            }
        }

        let mut text = format!("{:.1} h", *t / 3600.0);
        for (i, (ground, name)) in grounds.iter().zip(["Didymos", "Dimorphos"]).enumerate() {
            let surface = ground.t.row(0).to_vec();
            let (lo, hi) = surface.iter().fold((Float::MAX, Float::MIN), |(a, b), &x| (a.min(x), b.max(x)));
            text += &format!("   {name} {lo:.0}-{hi:.0} K");
            // Values ride the colour buffer, and go up on its flag.
            let mut mesh = sim.bodies[i].mesh.as_ref().unwrap().borrow_mut();
            mesh.values = surface;
            mesh.colors_dirty = true;
        }
        sim.huds[0].borrow_mut().text = text;
    });

    app.start();
}
