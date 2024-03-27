use crate::{
    compute_cosine_emission_angle, compute_cosine_incidence_angle, compute_cosine_phase_angle,
    config::Body, config::CfgScalar, config::Config, config::TemperatureInit,
    effective_temperature, flux_solar_radiation, newton_method_temperature, read_surface_low,
    shadows, update_colormap_scalar, util::*, AirlessBody, BodyData, FoldersRun, Routines, Surface,
    Time, Window, WindowScene,
};

use itertools::Itertools;
use polars::prelude::{df, CsvWriter, DataFrame, NamedFrom, SerWriter, Series};
use std::fs;

pub struct ThermalBodyData {
    pub depth_size: usize,
    pub cvg: usize,
    pub surf_low: Option<Surface>,
    pub albedos: DRVector<Float>,
    pub emissivities: DRVector<Float>,
    pub conductivities: DRVector<Float>,
    pub diffu_dx2: DMatrix<Float>,
    pub tmp: DMatrix<Float>,
    pub fluxes: DRVector<Float>,
    pub fluxes_solar: DRVector<Float>,
}

impl ThermalBodyData {
    pub fn new(asteroid: &AirlessBody, cb: &Body) -> Self {
        let depth_grid = &asteroid.interior.as_ref().unwrap().as_grid().depth;
        let depth_size = depth_grid.len();
        let surf_size = asteroid.surface.faces.len();

        let surf_low = cb
            .mesh_low
            .as_ref()
            .and_then(|_| Some(read_surface_low(cb).unwrap()));

        let albedos = DRVector::from_row_slice(
            &asteroid
                .surface
                .faces
                .iter()
                .map(|f| f.vertex.material.albedo)
                .collect_vec(),
        );
        let emissivities = DRVector::from_row_slice(
            &asteroid
                .surface
                .faces
                .iter()
                .map(|f| f.vertex.material.emissivity)
                .collect_vec(),
        );
        let conductivities = DRVector::from_row_slice(
            &asteroid
                .surface
                .faces
                .iter()
                .map(|f| f.vertex.material.conductivity())
                .collect_vec(),
        );
        let diffusivities = DMatrix::from_iterator(
            depth_size - 2,
            surf_size,
            (1..depth_size - 1)
                .map(|_| {
                    asteroid
                        .surface
                        .faces
                        .iter()
                        .map(|f| f.vertex.material.diffusivity())
                        .collect_vec()
                })
                .flatten(),
        );
        let dx2 = DRVector::from_row_slice(
            &depth_grid
                .iter()
                // .skip(1)
                .take(depth_size - 1)
                .tuple_windows()
                .map(|(a, b)| (b - a).powi(2))
                .collect_vec(),
        );
        let diffu_dx2 = DMatrix::from_columns(
            &diffusivities
                .column_iter()
                .map(|c| c.component_div(&dx2.transpose()))
                .collect_vec(),
        );

        let fluxes = DRVector::zeros(surf_size);
        let fluxes_solar = DRVector::zeros(surf_size);

        let tmp = DMatrix::<Float>::zeros(depth_size, surf_size);

        Self {
            depth_size,
            cvg: 3,
            surf_low,
            albedos,
            emissivities,
            conductivities,
            diffu_dx2,
            tmp,
            fluxes,
            fluxes_solar,
        }
    }
}

pub trait RoutinesThermal: Routines {
    fn fn_compute_initial_temperatures(
        &self,
        body: &AirlessBody,
        cb: &Body,
        sun_position: &Vec3,
    ) -> DMatrix<Float> {
        let depth_grid = &body.interior.as_ref().unwrap().as_grid().depth;
        let depth_size = depth_grid.len();
        let surf_size = body.surface.faces.len();

        match &cb.temperature {
            TemperatureInit::Effective(ratio) => {
                let ratio = ratio.unwrap_or((1, 4));
                let ratio = ratio.0 as Float / ratio.1 as Float;

                let mat = body.surface.faces[0].vertex.material;
                let init = effective_temperature(
                    sun_position.magnitude() * 1e3,
                    mat.albedo,
                    mat.emissivity,
                    ratio,
                );
                DMatrix::<Float>::from_element(depth_size, surf_size, init)
            }
            TemperatureInit::Scalar(scalar) => {
                DMatrix::<Float>::from_element(depth_size, surf_size, *scalar)
            }
            TemperatureInit::File(_p) => unimplemented!(),
        }
    }

    fn fn_compute_solar_flux(
        &self,
        body: &AirlessBody,
        body_data: &BodyData,
        thermal_data: &ThermalBodyData,
        sun_position: &Vec3,
    ) -> DRVector<Float> {
        let cos_incidences =
            compute_cosine_incidence_angle(body, &body_data.normals, &sun_position.normalize());
        flux_solar_radiation(
            &cos_incidences,
            &thermal_data.albedos,
            sun_position.magnitude() * 1e3 / AU,
        )
    }

    fn fn_compute_surface_temperatures(
        &self,
        body: &AirlessBody,
        thermal_data: &ThermalBodyData,
        surface_fluxes: &DRVector<Float>,
    ) -> DRVector<Float> {
        let depth_grid = &body.interior.as_ref().unwrap().as_grid().depth;

        newton_method_temperature(
            thermal_data.tmp.row(0).as_view(),
            &surface_fluxes,
            &thermal_data.emissivities,
            &thermal_data.conductivities,
            thermal_data.tmp.rows(1, 2).as_view(),
            depth_grid[1],
        )
    }

    fn fn_compute_heat_conduction(
        &self,
        body: &AirlessBody,
        thermal_data: &ThermalBodyData,
        delta_time: Float,
    ) -> DMatrix<Float> {
        let curr_size = thermal_data.depth_size - 2;
        let surf_size = body.surface.faces.len();

        let prev = thermal_data.tmp.view((0, 0), (curr_size, surf_size));
        let curr = thermal_data.tmp.view((1, 0), (curr_size, surf_size));
        let next = thermal_data.tmp.view((2, 0), (curr_size, surf_size));

        curr + delta_time
            * thermal_data
                .diffu_dx2
                .component_mul(&(prev - 2. * curr + next))
    }

    fn fn_compute_bottom_depth_temperatures(
        &self,
        _body: &AirlessBody,
        thermal_data: &ThermalBodyData,
    ) -> DRVector<Float> {
        thermal_data
            .tmp
            .row(thermal_data.depth_size - 2)
            .clone_owned()
    }
}

pub struct RoutinesThermalDefault {
    pub data: Vec<ThermalBodyData>,
    pub shadows_mutual: bool,
}

impl RoutinesThermalDefault {
    pub fn new() -> Self {
        Self {
            data: vec![],
            shadows_mutual: false,
        }
    }
}

impl Routines for RoutinesThermalDefault {
    fn load(&mut self, body: &AirlessBody, cb: &Body) {
        self.data.push(ThermalBodyData::new(body, cb));
    }

    // Sun position is obtained from the position of the light in window scene.
    // To avoid large floating values, distances are in km, so are the size of meshes and
    // position of Sun. The Sun distance will be converted to meters at the very last possible
    // moment. We keep Sun position vector in km for easier shadow computation with respect
    // to position of other bodies.
    // View factor does not require to be in meters as the result is a coefficient.
    // So keeping km should be ok.
    // To summarize, when you see vector position, expect it to be km, and distance should
    // be in meters for computation unless explicitely mentioned.
    fn fn_update_body_data(
        &mut self,
        config: &Config,
        body: usize,
        bodies: &mut [AirlessBody],
        bodies_data: &mut [BodyData],
        time: &Time,
        scene: &WindowScene,
    ) {
        let sun_position = scene.light.position;

        if time.iteration == 0 {
            let tmp = self.fn_compute_initial_temperatures(
                &bodies[body],
                &config.bodies[body],
                &sun_position,
            );
            self.data[body].tmp = tmp;
            return;
        }

        let dt = time.used_time_step();

        let other_bodies = (0..bodies.len()).filter(|ii| *ii != body).collect_vec();

        let mut fluxes_solar = self.fn_compute_solar_flux(
            &bodies[body],
            &bodies_data[body],
            &self.data[body],
            &sun_position,
        );

        if self.shadows_mutual {
            let shadows_self: Vec<usize> = vec![];
            let mut shadows_mutual: Vec<usize> = vec![];

            for other_body in other_bodies {
                shadows_mutual = shadows(&sun_position, &bodies[body], &bodies[other_body]);
            }

            for &index in shadows_mutual.iter().chain(&shadows_self).unique() {
                fluxes_solar[index] = 0.0;
            }
        }

        let fluxes = fluxes_solar.clone();

        let temperatures_surface =
            self.fn_compute_surface_temperatures(&bodies[body], &self.data[body], &fluxes);
        self.data[body].tmp.set_row(0, &temperatures_surface);

        let temperatures_inside =
            self.fn_compute_heat_conduction(&bodies[body], &self.data[body], dt as Float);
        let curr_size = self.data[body].depth_size - 2;
        for index in 0..curr_size {
            self.data[body]
                .tmp
                .set_row(index + 1, &temperatures_inside.row(index));
        }

        let temperatures_bottom =
            self.fn_compute_bottom_depth_temperatures(&bodies[body], &self.data[body]);
        self.data[body]
            .tmp
            .set_row(curr_size + 1, &temperatures_bottom);

        if let Some(true) = config.preferences.debug.thermal_stats {
            println!(
                "Update: {:.0} SF: {:.1}±({:.1})/{:.1}/{:.1} | T: {:.1}±({:.1})/{:.1}/{:.1}",
                time.elapsed_seconds(),
                fluxes_solar.mean(),
                fluxes_solar.variance().sqrt(),
                fluxes_solar.max(),
                fluxes_solar.min(),
                temperatures_surface.mean(),
                temperatures_surface.variance().sqrt(),
                temperatures_surface.max(),
                temperatures_surface.min()
            );
            println!(
                "more T0: {:.1}±({:.1})/{:.1}/{:.1} | T1: {:.1}±({:.1})/{:.1}/{:.1} | T2: {:.1}±({:.1})/{:.1}/{:.1}",
                self.data[body].tmp.row(0).mean(),
                self.data[body].tmp.row(0).variance().sqrt(),
                self.data[body].tmp.row(0).max(),
                self.data[body].tmp.row(0).min(),
                self.data[body].tmp.row(1).mean(),
                self.data[body].tmp.row(1).variance().sqrt(),
                self.data[body].tmp.row(1).max(),
                self.data[body].tmp.row(1).min(),
                self.data[body].tmp.row(2).mean(),
                self.data[body].tmp.row(2).variance().sqrt(),
                self.data[body].tmp.row(2).max(),
                self.data[body].tmp.row(2).min()
            );
        }

        self.data[body].fluxes = fluxes;
        self.data[body].fluxes_solar = fluxes_solar;
    }

    fn fn_update_body_colormap(
        &self,
        cfg: &Config,
        body: usize,
        bodies: &mut [AirlessBody],
        pre_computed_bodies: &[BodyData],
        _time: &Time,
        win: &Window,
    ) {
        let cmap = &cfg.window.colormap;
        let scalars = match &cmap.scalar {
            Some(CfgScalar::AngleIncidence) => compute_cosine_incidence_angle(
                &bodies[body],
                &pre_computed_bodies[body].normals,
                &win.scene.borrow().light.position.normalize(),
            )
            .map(|a| a.acos() * DPR),
            Some(CfgScalar::AngleEmission) => compute_cosine_emission_angle(
                &bodies[body],
                &pre_computed_bodies[body].normals,
                &win.scene.borrow().camera.position.normalize(),
            )
            .map(|a| a.acos() * DPR),
            Some(CfgScalar::AnglePhase) => compute_cosine_phase_angle(
                &bodies[body],
                &win.scene.borrow().camera.position.normalize(),
                &win.scene.borrow().light.position.normalize(),
            )
            .map(|a| a.acos() * DPR),
            Some(CfgScalar::FluxSolar) => self.data[body].fluxes_solar.clone(),
            Some(CfgScalar::FluxSurface) => self.data[body].fluxes.clone(),
            Some(CfgScalar::FluxEmitted) => unimplemented!(),
            Some(CfgScalar::FluxSelf) => unimplemented!(),
            Some(CfgScalar::FluxMutual) => unimplemented!(),
            Some(CfgScalar::File) => unimplemented!(),
            None | Some(CfgScalar::Temperature) => self.data[body].tmp.row(0).into_owned(),
        };

        update_colormap_scalar(win, cfg, scalars.as_slice(), &mut bodies[body], body);
    }

    fn fn_export_iteration(
        &self,
        body: usize,
        cfg: &Config,
        time: &Time,
        folders: &FoldersRun,
        is_first_it: bool,
    ) {
        let np_elapsed = time.elapsed_seconds() as Float / cfg.bodies[body].spin.period;

        let data = &self.data[body];
        let tmp_surf_min = data.tmp.row(0).min();
        let tmp_surf_max = data.tmp.row(0).max();
        let tmp_surf_mean = data.tmp.row(0).mean();
        let tmp_surf_stdev = data.tmp.row(0).variance().sqrt();
        let tmp_bottom_min = data.tmp.row(data.depth_size - 1).min();
        let tmp_bottom_max = data.tmp.row(data.depth_size - 1).max();
        let tmp_bottom_mean = data.tmp.row(data.depth_size - 1).mean();
        let tmp_bottom_stdev = data.tmp.row(data.depth_size - 1).variance().sqrt();

        let mut df = df!(
            "time" => [np_elapsed],
            "tmp-surf-min" => [tmp_surf_min],
            "tmp-surf-max" => [tmp_surf_max],
            "tmp-surf-mean" => [tmp_surf_mean],
            "tmp-surf-stdev" => [tmp_surf_stdev],
            "tmp-bottom-min" => [tmp_bottom_min],
            "tmp-bottom-max" => [tmp_bottom_max],
            "tmp-bottom-mean" => [tmp_bottom_mean],
            "tmp-bottom-stdev" => [tmp_bottom_stdev],
        )
        .unwrap();

        let folder_simu = folders.simu_body(&cfg.bodies[body].name);
        fs::create_dir_all(&folder_simu).unwrap();

        let mut file = std::fs::File::options()
            .append(true)
            .create(true)
            .open(folder_simu.join("progress.csv"))
            .unwrap();
        CsvWriter::new(&mut file)
            .include_header(is_first_it)
            .finish(&mut df)
            .unwrap();
    }

    fn fn_export_iteration_period(
        &self,
        body: usize,
        _bodies: &mut [AirlessBody],
        cfg: &Config,
        folders: &FoldersRun,
        exporting_started_elapsed: i64,
        is_first_it_export: bool,
    ) {
        let folder_tpm = folders.simu_rec_time_body_temperatures(
            exporting_started_elapsed as _,
            &cfg.bodies[body].name,
        );
        fs::create_dir_all(&folder_tpm).unwrap();

        let data = &self.data[body];

        if is_first_it_export {
            let mut df = df!(
                "tmp" => data.tmp.map(|t| t as f32).as_slice(),
            )
            .unwrap();
            let mut file = std::fs::File::options()
                .append(true)
                .create(true)
                .open(folder_tpm.join("temperatures-all.csv"))
                .unwrap();
            CsvWriter::new(&mut file).finish(&mut df).unwrap();
        }

        if !cfg.bodies[body].record.faces.is_empty() {
            let dfcols = cfg.bodies[body]
                .record
                .faces
                .iter()
                .map(|&face| Series::new(&format!("{}", face), &vec![data.tmp.row(0)[face]]))
                .collect_vec();
            let mut df = DataFrame::new(dfcols).unwrap();
            let p = folder_tpm.join("temperatures-faces.csv");
            let mut file = std::fs::File::options()
                .append(true)
                .create(true)
                .open(&p)
                .unwrap();
            CsvWriter::new(&mut file)
                .include_header(is_first_it_export)
                .finish(&mut df)
                .unwrap();
        }

        if !cfg.bodies[body].record.cells.is_empty() {
            let dfcols = cfg.bodies[body]
                .record
                .cells
                .iter()
                .map(|&cell| Series::new(&format!("{}", cell), &vec![data.tmp[cell]]))
                .collect_vec();
            let mut df = DataFrame::new(dfcols).unwrap();
            let p = folder_tpm.join("temperatures-cells.csv");
            let mut file = std::fs::File::options()
                .append(true)
                .create(true)
                .open(&p)
                .unwrap();
            CsvWriter::new(&mut file)
                .include_header(is_first_it_export)
                .finish(&mut df)
                .unwrap();
        }

        if !cfg.bodies[body].record.columns.is_empty() {
            let dfcols = cfg.bodies[body]
                .record
                .columns
                .iter()
                .map(|&column| {
                    Series::new(&format!("{}", column), data.tmp.column(column).as_slice())
                })
                .collect_vec();
            let mut df = DataFrame::new(dfcols).unwrap();
            let p = folder_tpm.join("temperatures-columns.csv");
            let mut file = std::fs::File::options()
                .append(true)
                .create(true)
                .open(&p)
                .unwrap();
            CsvWriter::new(&mut file)
                .include_header(is_first_it_export)
                .finish(&mut df)
                .unwrap();
        }

        if !cfg.bodies[body].record.rows.is_empty() {
            let dfcols = cfg.bodies[body]
                .record
                .rows
                .iter()
                .map(|&row| {
                    Series::new(
                        &format!("{}", row),
                        data.tmp.row(row).transpose().as_slice(),
                    )
                })
                .collect_vec();
            let mut df = DataFrame::new(dfcols).unwrap();
            let p = folder_tpm.join("temperatures-rows.csv");
            let mut file = std::fs::File::options()
                .append(true)
                .create(true)
                .open(&p)
                .unwrap();
            CsvWriter::new(&mut file)
                .include_header(is_first_it_export)
                .finish(&mut df)
                .unwrap();
        }
    }
}

impl RoutinesThermal for RoutinesThermalDefault {}
