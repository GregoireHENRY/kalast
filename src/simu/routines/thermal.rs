use crate::{
    compute_cosine_emission_angle, compute_cosine_incidence_angle, compute_cosine_phase_angle,
    config::{Body, CfgColormap, CfgScalar, Config, TemperatureInit},
    effective_temperature, flux_solar_radiation, newton_method_temperature, read_surface_low,
    util::*,
    AirlessBody, BodyData, ColorMode, FacetColorChanged, FoldersRun, Routines, Surface, Time,
    Window,
};

use itertools::Itertools;
use polars::prelude::{
    df, CsvReader, CsvWriter, DataFrame, NamedFrom, SerReader, SerWriter, Series,
};
use std::{
    collections::{HashMap, HashSet},
    fs,
};

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
    pub view_factors: HashMap<usize, DMatrix<Float>>,
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
            view_factors: HashMap::new(),
        }
    }
}

pub trait RoutinesThermal: Routines {
    fn fn_compute_initial_temperatures(
        &self,
        config: &Config,
        bodies: &[AirlessBody],
        body: usize,
        sun_position: &Vec3,
        time: &Time,
    ) -> DMatrix<Float> {
        let depth_grid = &bodies[body].interior.as_ref().unwrap().as_grid().depth;
        let depth_size = depth_grid.len();
        let surf_size = bodies[body].surface.faces.len();

        if let Some(restart) = config.restart.as_ref() {
            let folders = FoldersRun::new(restart.path.as_ref().unwrap());
            let p = folders
                .simu_rec_time_body_temperatures(time.elapsed_time, &config.bodies[body].name)
                .join("temperatures-all.csv");

            dbg!(&p);

            let df = CsvReader::from_path(p).unwrap().finish().unwrap();

            // Temperatures exported as f32 to save space but read as f64.
            DMatrix::<Float>::from_iterator(
                depth_size,
                surf_size,
                df.column("tmp")
                    .unwrap()
                    .f64()
                    .unwrap()
                    .into_iter()
                    .map(|t| t.unwrap()),
            )
        } else {
            let cb = &config.bodies[body];
            match &cb.temperature {
                TemperatureInit::Effective(ratio) => {
                    let ratio = ratio.unwrap_or((1, 4));
                    let ratio = ratio.0 as Float / ratio.1 as Float;

                    let mat = bodies[body].surface.faces[0].vertex.material;
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
}

impl RoutinesThermalDefault {
    pub fn new() -> Self {
        Self { data: vec![] }
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
        sun: &Vec3,
        time: &Time,
        _window: Option<&Window>,
    ) {
        if time.iteration == 0 {
            let tmp = self.fn_compute_initial_temperatures(config, bodies, body, sun, time);
            self.data[body].tmp = tmp;
            return;
        }

        let dt = time.used_time_step();

        let mut fluxes_solar =
            self.fn_compute_solar_flux(&bodies[body], &bodies_data[body], &self.data[body], sun);

        let mut other_bodies_shadowing = HashSet::new();

        if let Some(true) = config.simulation.mutual_shadowing.as_ref() {
            other_bodies_shadowing.extend(0..bodies.len());
        }

        match config.simulation.self_shadowing.as_ref() {
            None | Some(false) => {
                other_bodies_shadowing.remove(&body);
            }
            _ => {}
        };

        let mut other_bodies_heating = HashSet::new();

        if let Some(true) = config.simulation.mutual_heating.as_ref() {
            other_bodies_heating.extend(0..bodies.len());
        }

        match config.simulation.self_heating.as_ref() {
            None | Some(false) => {
                other_bodies_heating.remove(&body);
            }
            _ => {}
        };

        let mut shadows = HashSet::new();

        for other_body in other_bodies_shadowing {
            let shadows_mutual = crate::shadows(sun, &bodies[body], &bodies[other_body]);
            shadows.extend(shadows_mutual);
        }

        for index in shadows {
            fluxes_solar[index] = 0.0;
        }

        self.data[body].fluxes_solar = fluxes_solar.clone();

        let mut fluxes = fluxes_solar;

        for other_body in other_bodies_heating {
            // maybe mut
            let vfs = crate::view_factor(&bodies[body], &bodies[other_body], true);

            if self.data[body].view_factors.contains_key(&other_body) {
                self.data[body].view_factors.remove(&other_body);
            }
            self.data[body].view_factors.insert(other_body, vfs.clone());

            // Pas sûr...
            // for (mut vf, face) in izip!(vfs.column_iter_mut(), &b1.surface.faces) {
            //     vf *= face.area;
            // }
            // let vfmax = vfs.max();

            let diffuse = crate::diffuse_solar_radiation(
                &vfs,
                &self.data[other_body].fluxes_solar,
                &self.data[other_body].albedos,
            );

            let direct = crate::direct_thermal_heating(
                &vfs,
                &self.data[other_body].tmp.row(0).clone_owned(),
                &self.data[other_body].emissivities,
            );

            fluxes += diffuse + direct;
        }

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
                self.data[body].fluxes_solar.mean(),
                self.data[body].fluxes_solar.variance().sqrt(),
                self.data[body].fluxes_solar.max(),
                self.data[body].fluxes_solar.min(),
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
    }

    fn fn_compute_body_colormap(
        &self,
        _config: &Config,
        body: usize,
        bodies: &mut [AirlessBody],
        bodies_data: &[BodyData],
        _time: &Time,
        win: &Window,
        cmap: &CfgColormap,
    ) -> Option<DRVector<Float>> {
        match &cmap.scalar {
            Some(CfgScalar::AngleIncidence) => Some(
                compute_cosine_incidence_angle(
                    &bodies[body],
                    &bodies_data[body].normals,
                    &win.scene.borrow().light.position.normalize(),
                )
                .map(|a| a.acos() * DPR),
            ),
            Some(CfgScalar::AngleEmission) => Some(
                compute_cosine_emission_angle(
                    &bodies[body],
                    &bodies_data[body].normals,
                    &win.scene.borrow().camera.position.normalize(),
                )
                .map(|a| a.acos() * DPR),
            ),
            Some(CfgScalar::AnglePhase) => Some(
                compute_cosine_phase_angle(
                    &bodies[body],
                    &win.scene.borrow().camera.position.normalize(),
                    &win.scene.borrow().light.position.normalize(),
                )
                .map(|a| a.acos() * DPR),
            ),
            Some(CfgScalar::FluxSolar) => Some(self.data[body].fluxes_solar.clone()),
            Some(CfgScalar::FluxSurface)
            | Some(CfgScalar::FluxEmitted)
            | Some(CfgScalar::FluxSelf)
            | Some(CfgScalar::FluxMutual)
            | Some(CfgScalar::File) => Some(self.data[body].fluxes.clone()),
            None | Some(CfgScalar::Temperature) => Some(self.data[body].tmp.row(0).into_owned()),
            Some(CfgScalar::ViewFactor) => {
                // move than to thermal impl of fn on selected
                // if none of that, changed all facets to diffuse, change them back when on selected
                None
            }
        }
    }

    fn fn_render_on_facet_selected(
        &mut self,
        config: &Config,
        body: usize,
        _face: usize,
        bodies: &mut [AirlessBody],
        bodies_data: &mut [BodyData],
        window: &Window,
        _time: &Time,
    ) {
        // Goal here is to have an entry point in Thermal Routines for showing view-factor when a facet is
        // clicked.
        //
        // Once a facet is clicked, this function is called. This function searches for the first selected
        // facet for the body clicked-on to color the facets of the other body contributing in view-factor
        // to the identified facet.
        //
        // This function can also be called when a facet is clicked for the second time and then
        // de-selected and maybe no facet anymore is selected. In this case, no view-factor should be
        // displayed.
        //
        // All facets not displaying view-factor should display diffuse lighting.
        let other_body = (body + 1).rem_euclid(2);

        println!("Body: {}, other body: {}", body, other_body);

        if let Some(first_selected) = bodies_data[body].facets_selected.first().map(|f| f.index) {
            // let first_selected_other = bodies_data[other_body].selected.first().unwrap().index;

            println!("First selected facet: {}", first_selected);

            if let (Some(cmap), Some(true)) = (
                config.window.colormap.as_ref(),
                config.window.selecting_facet_shows_view_factor,
            ) {
                if window.is_paused() {
                    println!("Cmap found, window paused and selecting facets to show view factor is enabled!");

                    match &cmap.scalar {
                        Some(CfgScalar::ViewFactor) => {
                            println!("Cmap scalar is view-factor");

                            let scalar = self.data[other_body]
                                .view_factors
                                .get(&body)
                                .unwrap()
                                .column(first_selected)
                                .transpose()
                                .into_owned();

                            println!("Scalar length: {}", scalar.len());
                            println!(
                                "Mean: {}, max: {}, min: {}, std: {}",
                                scalar.mean(),
                                scalar.max(),
                                scalar.min(),
                                scalar.variance().sqrt()
                            );

                            // Register facets that will be colored to be able to revert them later.
                            for face in 0..scalar.len() {
                                if scalar[face] > 0.0 {
                                    let test = bodies_data[other_body]
                                        .facets_showing_view_factor
                                        .iter()
                                        .any(|f| f.index == face);
                                    println!("vf #{} = {}", face, scalar[face]);
                                    if !test {
                                        let facet_changed =
                                            FacetColorChanged::set(&bodies[other_body], face);
                                        bodies_data[other_body]
                                            .facets_showing_view_factor
                                            .push(facet_changed);
                                        bodies[other_body].surface.faces[face].vertex.color_mode =
                                            ColorMode::Data;
                                    }
                                    bodies[other_body].surface.faces[face].vertex.data =
                                        scalar[face];
                                }
                            }

                            println!(
                                "Facets colored for view-factor: {}",
                                bodies_data[other_body].facets_showing_view_factor.len()
                            );

                            // Then it will be render later in routine calling this function.
                        }
                        None | Some(_) => {}
                    };
                }
            }
        } else {
            println!("All facets de-selected.");

            // No facet selected. Happens when de-selecting last facet.
            // Here we revert all changed facets back to how they were.
            for face in 0..bodies_data[other_body].facets_showing_view_factor.len() {
                let facet_changed = &bodies_data[other_body].facets_showing_view_factor[face];
                let v = &mut bodies[other_body].surface.faces[facet_changed.index].vertex;
                v.color_mode = facet_changed.mode;
                match facet_changed.mode {
                    ColorMode::Color => v.color = facet_changed.color,
                    _ => {}
                };
            }
            bodies_data[other_body].facets_showing_view_factor.clear();
        }
        println!("End fn_render_on_facet_selected Thermal routines");
        println!("");
    }

    fn fn_export_iteration(&self, body: usize, cfg: &Config, time: &Time, folders: &FoldersRun) {
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

        let path = folder_simu.join("progress.csv");
        let exists = path.exists();

        let mut file = std::fs::File::options()
            .append(true)
            .create(true)
            .open(path)
            .unwrap();
        CsvWriter::new(&mut file)
            .include_header(!exists)
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

        if let Some(record) = cfg.bodies[body].record.as_ref() {
            if let Some(faces) = record.faces.as_ref() {
                let dfcols = faces
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

            if let Some(cells) = record.cells.as_ref() {
                let dfcols = cells
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

            if let Some(columns) = record.columns.as_ref() {
                let dfcols = columns
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

            if let Some(rows) = record.rows.as_ref() {
                let dfcols = rows
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
}

impl RoutinesThermal for RoutinesThermalDefault {}
