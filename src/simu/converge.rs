use crate::{config::Body, config::CfgTimeExport, util::*, AirlessBody, ThermalBodyData};

use itertools::izip;
use notify_rust::Notification;
use polars::prelude::{
    df, CsvReader, CsvWriter, DataFrame, NamedFrom, SerReader, SerWriter, Series,
};
use std::path::Path;

pub fn check<P: AsRef<Path>>(
    id: usize,
    asteroid: &mut AirlessBody,
    cb: &Body,
    info: &mut ThermalBodyData,
    path: P,
    ct: &CfgTimeExport,
) -> bool {
    let path = path.as_ref();

    /*
    let p = path.join("scal.csv");
    let df = CsvReader::from_path(p).unwrap().finish().unwrap();
    */

    let time_elapsed: Float = path
        .parent()
        .unwrap()
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .parse()
        .unwrap();

    let p = path.join("temperatures").join("columns.csv");
    let df = CsvReader::from_path(p).unwrap().finish().unwrap();

    let zdepth = &asteroid.interior.as_ref().unwrap().as_grid().depth;

    let len_time = (ct.duration / ct.step) as usize + 1;
    let len_spin = (cb.spin.period / ct.step as Float).ceil() as usize + 1;
    let n_spins = (ct.duration as Float / cb.spin.period).floor() as usize;
    let n_spins_elapsed = time_elapsed / cb.spin.period;

    let c0_ii = cb.record.columns[0];
    let c0_name = format!("{}", c0_ii);

    let tsd = asteroid.surface.faces[c0_ii]
        .vertex
        .material
        .thermal_skin_depth_one(cb.spin.period);
    let ii_tsd_next = zdepth.iter().position(|&x| x > tsd).unwrap();

    let c0 = df.column(&c0_name).unwrap().f64().unwrap();
    let tmp =
        DMatrix::<Float>::from_iterator(zdepth.len(), len_time, c0.into_iter().map(|t| t.unwrap()))
            .transpose();

    let mut m = DMatrix::<Float>::zeros(zdepth.len(), n_spins * 2);
    let mut dfcols = vec![Series::new("depth", zdepth.as_slice())];

    let mut ii_start = 0;

    for ii_spin in 0..n_spins {
        let odd = ii_spin % 2;
        let ii_len = len_spin + odd;
        let y = tmp.rows(ii_start, ii_len);

        for (ii, col) in y.column_iter().enumerate() {
            m[(ii, ii_spin * 2 + 0)] = col.mean();
            m[(ii, ii_spin * 2 + 1)] = col.variance().sqrt();
        }

        dfcols.push(Series::new(
            &format!("{}-{}-mean", c0_ii, ii_spin),
            m.column(ii_spin * 2 + 0).as_slice(),
        ));

        dfcols.push(Series::new(
            &format!("{}-{}-std", c0_ii, ii_spin),
            m.column(ii_spin * 2 + 1).as_slice(),
        ));

        ii_start += ii_len - 2;
    }

    let mut df = DataFrame::new(dfcols).unwrap();
    let mut file = std::fs::File::options()
        .append(true)
        .create(true)
        .open(path.parent().unwrap().join(format!("tmp-cvg-{}.csv", id)))
        .unwrap();
    CsvWriter::new(&mut file).finish(&mut df).unwrap();

    let mut converged = true;
    let ii_period = n_spins - 1;

    let body_cvg = info.cvg;

    if body_cvg > 0 {
        let mut cvg = 0;
        let mut ii_depth_cvgd = 0;

        for _ in 1..body_cvg + 1 {
            cvg += 1;
            converged = true;
            ii_depth_cvgd = 0;

            if m[(0, 2 * ii_period + 1)] >= m[(0, 2 * ii_period)] || n_spins_elapsed < 2.0 {
                converged = false;
                break;
            }

            for ii in 0..ii_tsd_next {
                if m[(ii, 2 * ii_period + 1)] * cvg as Float
                    <= (m[(ii + 1, 2 * ii_period)] - m[(ii, 2 * ii_period)]).abs()
                    && m[(ii, 2 * ii_period + 1)] >= 1e-3
                {
                    converged = false;
                    break;
                } else {
                    ii_depth_cvgd += 1;
                }
            }

            if converged {
                break;
            }
        }

        let mut df = df!(
            "level" => [cvg as u32],
            "cvg" => [zdepth[ii_depth_cvgd]],
        )
        .unwrap();

        let mut file = std::fs::File::options()
            .append(true)
            .create(true)
            .open(
                path.parent()
                    .unwrap()
                    .join(format!("tmp-cvg-summary-{}.csv", id)),
            )
            .unwrap();
        CsvWriter::new(&mut file).finish(&mut df).unwrap();

        if converged {
            let folder_simu = path.ancestors().nth(4).unwrap();

            let mut df = df!(
                "time" => [n_spins_elapsed],
                "event" => [format!("cvg-{}σ", cvg)],
            )
            .unwrap();

            let mut file = std::fs::File::options()
                .append(true)
                .create(true)
                .open(folder_simu.join(format!("events-{}.csv", id)))
                .unwrap();
            CsvWriter::new(&mut file)
                .include_header(body_cvg == 3)
                .finish(&mut df)
                .unwrap();

            Notification::new()
                .summary(&format!(
                    "Kalast body-{} cvg-{}σ @{:.0}",
                    id, cvg, n_spins_elapsed
                ))
                .body("")
                .show()
                .unwrap();

            // body_cvg = cvg - 1;
            info.cvg = cvg - 1;
        }
    }

    converged
}

pub fn check_all<P: AsRef<Path>>(
    asteroids: &mut [AirlessBody],
    cbs: &[Body],
    infos: &mut [ThermalBodyData],
    path: P,
    ct: &CfgTimeExport,
) -> bool {
    let path = path.as_ref();
    let mut converged_all = true;

    for (id, (asteroid, cb, info)) in izip!(asteroids, cbs, infos).enumerate() {
        let is_record = !cb.record.columns.is_empty();

        if is_record {
            let path_body = path.join(format!("body-{}", id));
            converged_all &= check(id, asteroid, cb, info, path_body, ct);
        }
    }

    converged_all
}
