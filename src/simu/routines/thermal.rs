use crate::{
    compute_cosine_emission_angle, compute_cosine_incidence_angle, compute_cosine_phase_angle,
    effective_temperature, flux_solar_radiation, newton_method_temperature, read_surface_low,
    shadows, simu::Scene, update_colormap_scalar, util::*, AirlessBody, Cfg, CfgBody, CfgScalar,
    CfgTemperatureInit, FoldersRun, PreComputedBody, Routines, RoutinesData, Surface,
    Time, Window,
};

use itertools::Itertools;
use polars::prelude::{df, CsvWriter, DataFrame, NamedFrom, SerWriter, Series};
use std::fs;

pub struct ThermalData {
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

impl RoutinesData for ThermalData {
    fn new(asteroid: &AirlessBody, cb: &CfgBody, scene: &Scene) -> Self {
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
                .skip(1)
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

        let tmp = match &cb.temperature {
            CfgTemperatureInit::Effective(ratio) => {
                let ratio = ratio.unwrap_or((1, 4));
                let ratio = ratio.0 as Float / ratio.1 as Float;

                let mat = asteroid.surface.faces[0].vertex.material;
                let init = effective_temperature(&scene.sun_pos, mat.albedo, mat.emissivity, ratio);
                DMatrix::<Float>::from_element(depth_size, surf_size, init)
            }
            CfgTemperatureInit::Scalar(scalar) => {
                DMatrix::<Float>::from_element(depth_size, surf_size, *scalar)
            }
            CfgTemperatureInit::File(_p) => unimplemented!(),
        };

        let fluxes = DRVector::zeros(surf_size);
        let fluxes_solar = DRVector::zeros(surf_size);

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
    fn fn_compute_solar_flux(
        &self,
        body: &AirlessBody,
        precomputed: &PreComputedBody,
        body_info: &ThermalData,
        scene: &Scene,
    ) -> DRVector<Float>;

    fn fn_compute_surface_temperatures(
        &self,
        body: &AirlessBody,
        body_info: &ThermalData,
        surface_fluxes: &DRVector<Float>,
    ) -> DRVector<Float>;

    fn fn_compute_heat_conduction(
        &self,
        body: &AirlessBody,
        body_info: &ThermalData,
        delta_time: Float,
    ) -> DMatrix<Float>;

    fn fn_compute_bottom_depth_temperatures(
        &self,
        body: &AirlessBody,
        body_info: &ThermalData,
    ) -> DRVector<Float>;
}

pub struct RoutinesThermalDefault {
    pub data: Vec<ThermalData>,
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
    fn fn_iteration_body(
        &mut self,
        body: usize,
        bodies: &mut [AirlessBody],
        pre_computed_bodies: &mut [PreComputedBody],
        time: &Time,
        scene: &Scene,
    ) {
        let dt = time.used_time_step();

        let other_bodies = (0..bodies.len()).filter(|ii| *ii != body).collect_vec();

        let mut fluxes_solar = self.fn_compute_solar_flux(
            &bodies[body],
            &pre_computed_bodies[body],
            &self.data[body],
            &scene,
        );

        if self.shadows_mutual {
            let shadows_self: Vec<usize> = vec![];
            let mut shadows_mutual: Vec<usize> = vec![];

            for other_body in other_bodies {
                shadows_mutual = shadows(&scene.sun_pos, &bodies[body], &bodies[other_body]);
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

        self.data[body].fluxes = fluxes;
        self.data[body].fluxes_solar = fluxes_solar;
    }

    fn fn_update_colormap(
        &self,
        body: usize,
        bodies: &mut [AirlessBody],
        pre_computed_bodies: &[PreComputedBody],
        cfg: &Cfg,
        scene: &Scene,
        win: &Window,
    ) {
        let cmap = &cfg.window.colormap;
        let scalars = match &cmap.scalar {
            Some(CfgScalar::AngleIncidence) => compute_cosine_incidence_angle(
                &bodies[body],
                &pre_computed_bodies[body].normals,
                scene,
            )
            .map(|a| a.acos() * DPR),
            Some(CfgScalar::AngleEmission) => compute_cosine_emission_angle(
                &bodies[body],
                &pre_computed_bodies[body].normals,
                scene,
            )
            .map(|a| a.acos() * DPR),
            Some(CfgScalar::AnglePhase) => {
                compute_cosine_phase_angle(&bodies[body], scene).map(|a| a.acos() * DPR)
            }
            Some(CfgScalar::FluxSolar) => self.data[body].fluxes_solar.clone(),
            Some(CfgScalar::FluxSurface) => self.data[body].fluxes.clone(),
            Some(CfgScalar::FluxEmitted) => unimplemented!(),
            Some(CfgScalar::FluxSelf) => unimplemented!(),
            Some(CfgScalar::FluxMutual) => unimplemented!(),
            None | Some(CfgScalar::Temperature) => self.data[body].tmp.row(0).into_owned(),
        };

        update_colormap_scalar(win, cfg, scalars.as_slice(), &mut bodies[body], body);
    }

    fn fn_export_iteration(
        &self,
        body: usize,
        cfg: &Cfg,
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

        let folder_simu = folders.simu_body(&cfg.bodies[body].id);
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
        cfg: &Cfg,
        folders: &FoldersRun,
        exporting_started_elapsed: i64,
        is_first_it_export: bool,
    ) {
        let folder_tpm = folders
            .simu_rec_time_body_temperatures(exporting_started_elapsed as _, &cfg.bodies[body].id);
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

impl RoutinesThermal for RoutinesThermalDefault {
    fn fn_compute_solar_flux(
        &self,
        body: &AirlessBody,
        precomputed: &PreComputedBody,
        body_info: &ThermalData,
        scene: &Scene,
    ) -> DRVector<Float> {
        let cos_incidences = compute_cosine_incidence_angle(body, &precomputed.normals, scene);
        flux_solar_radiation(&cos_incidences, &body_info.albedos, scene.sun_dist())
    }

    fn fn_compute_surface_temperatures(
        &self,
        body: &AirlessBody,
        body_info: &ThermalData,
        surface_fluxes: &DRVector<Float>,
    ) -> DRVector<Float> {
        let depth_grid = &body.interior.as_ref().unwrap().as_grid().depth;

        newton_method_temperature(
            body_info.tmp.row(0).as_view(),
            &surface_fluxes,
            &body_info.emissivities,
            &body_info.conductivities,
            body_info.tmp.rows(1, 2).as_view(),
            depth_grid[2],
        )
    }

    fn fn_compute_heat_conduction(
        &self,
        body: &AirlessBody,
        body_info: &ThermalData,
        delta_time: Float,
    ) -> DMatrix<Float> {
        let curr_size = body_info.depth_size - 2;
        let surf_size = body.surface.faces.len();

        let prev = body_info.tmp.view((0, 0), (curr_size, surf_size));
        let curr = body_info.tmp.view((1, 0), (curr_size, surf_size));
        let next = body_info.tmp.view((2, 0), (curr_size, surf_size));

        curr + delta_time
            * body_info
                .diffu_dx2
                .component_mul(&(prev - 2. * curr + next))
    }

    fn fn_compute_bottom_depth_temperatures(
        &self,
        _body: &AirlessBody,
        body_info: &ThermalData,
    ) -> DRVector<Float> {
        body_info.tmp.row(body_info.depth_size - 2).clone_owned()
    }
}
