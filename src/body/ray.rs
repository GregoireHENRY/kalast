use crate::{util::*, vec3_to_4_one, AirlessBody};

use itertools::izip;

fn point_in_or_on(p1: &Vec3, p2: &Vec3, a: &Vec3, b: &Vec3) -> bool {
    let cp1 = (b - a).cross(&(p1 - a));
    let cp2 = (b - a).cross(&(p2 - a));
    cp1.dot(&cp2) >= 0.0
}

fn point_in_or_on_triangle(p: &Vec3, a: &Vec3, b: &Vec3, c: &Vec3) -> bool {
    point_in_or_on(p, a, b, c) && point_in_or_on(p, b, c, a) && point_in_or_on(p, c, a, b)
}

pub fn intersect_plane(raystart: &Vec3, raydir: &Vec3, a: &Vec3, b: &Vec3, c: &Vec3) -> Vec3 {
    let normal = (b - a).cross(&(c - a));
    raystart + raydir * (a - raystart).dot(&normal) / raydir.dot(&normal)
}

pub fn intersect_triangle(
    raystart: &Vec3,
    raydir: &Vec3,
    a: &Vec3,
    b: &Vec3,
    c: &Vec3,
) -> Option<Vec3> {
    let x = intersect_plane(raystart, raydir, a, b, c);
    point_in_or_on_triangle(&x, a, b, c).then_some(x)
}

// Everything is expressed in surf reference frame.
pub fn intersect_surface(
    raystart: &Vec3,
    raydir: &Vec3,
    surf: Vec<(&Vec3, &Vec3, &Vec3)>,
    exit_first: bool,
) -> Option<(Vec3, usize)> {
    let mut best_intersect: Option<(Vec3, usize)> = None;

    for (index, face) in surf.iter().enumerate() {
        if let Some(intersect) = intersect_triangle(raystart, raydir, &face.0, &face.1, &face.2) {
            /*
            println!(
                "intersected #{} start: {:?}, intersected: {:?}, dist: {}, best dist found: {:?}",
                index,
                raystart.as_slice(),
                intersect.as_slice(),
                (intersect - raystart).magnitude(),
                best_intersect
                    .as_ref()
                    .and_then(|i| Some((i.0 - raystart).magnitude()))
            );
            */

            if best_intersect.is_none()
                || (intersect - raystart).magnitude()
                    < (best_intersect.as_ref().unwrap().0 - raystart).magnitude()
            {
                best_intersect = Some((intersect, index));

                if exit_first {
                    return best_intersect;
                }
            }
        };
    }

    best_intersect
}

pub fn intersect_asteroids(
    raystart_world: &Vec3,
    raydir_world: &Vec3,
    asteroids: &[AirlessBody],
) -> Option<(Vec3, usize, usize)> {
    let mut best_intersect: Option<(Vec3, usize, usize)> = None;

    for (surface_index, asteroid) in asteroids.iter().enumerate() {
        let raystart =
            (glm::inverse(&asteroid.matrix_model) * vec3_to_4_one(&raystart_world)).xyz();

        let rayend = (glm::inverse(&asteroid.matrix_model)
            * vec3_to_4_one(&(raystart_world + raydir_world)))
        .xyz();
        let raydir = (rayend - raystart).normalize();

        /*
        println!(
            "{}> raystart model: {:?}",
            surface_index,
            raystart.as_slice()
        );
        println!("{}> raydir model: {:?}", surface_index, raydir.as_slice());
        println!("{}> matrix model: {}", surface_index, asteroid.matrix_model);
        */

        let surf = asteroid
            .surface
            .faces_vertices()
            .iter()
            .map(|f| (&f.0.position, &f.1.position, &f.2.position))
            .collect();
        if let Some((intersect, face_index)) = intersect_surface(&raystart, &raydir, surf, false) {
            if best_intersect.is_none()
                || (intersect - raystart).magnitude()
                    < (best_intersect.as_ref().unwrap().0 - raystart).magnitude()
            {
                best_intersect = Some((intersect, face_index, surface_index));
            }
        }
    }

    best_intersect
}

/// asteroid1 is shadowed by asteroid2.
/// Distances are kept to km in this routine for there is no need of conversion to meters.
pub fn shadows(
    sun_position: &Vec3,
    asteroid1: &AirlessBody,
    asteroid2: &AirlessBody,
) -> Vec<usize> {
    if asteroid1.surface.is_smooth() || asteroid2.surface.is_smooth() {
        unimplemented!("Shadowing by ray-casting is only implemented for flat surface.");
    }

    let mut shadowed = vec![];

    // Calculation of distance in frame of world.
    let pos1 = asteroid1.matrix_model.column(3).xyz();
    let pos2 = asteroid2.matrix_model.column(3).xyz();
    let d1 = (pos1 - sun_position).magnitude();
    let d2 = (pos2 - sun_position).magnitude();

    if d2 > d1 {
        return shadowed;
    }

    // Inverse of model matrices to go from world to asteroids frame.
    let inv_m2 = glm::inverse(&asteroid2.matrix_model);
    let inv_m1 = glm::inverse(&asteroid1.matrix_model);

    // From asteroid1 to asteroid2.
    let inv_m2_m1 = inv_m2 * asteroid1.matrix_model;

    // In frame of asteroids.
    let lightpos_f2 = (inv_m2 * vec3_to_4_one(&sun_position)).xyz();
    let lightpos_f1 = (inv_m1 * vec3_to_4_one(&sun_position)).xyz();

    // Filtering out faces that cannot be shadowed because they don't see the Sun.
    for (index, face) in asteroid1
        .surface
        .faces
        .iter()
        .enumerate()
        .filter(|(_, f)| f.vertex.normal.angle(&lightpos_f1) < PI / 2.0)
    {
        // In frame of asteroid2.
        let _area_sqrt = face.area.sqrt();
        let center_f2 = (inv_m2_m1 * vec3_to_4_one(&face.vertex.position)).xyz();
        let raydist = (center_f2 - lightpos_f2).magnitude();
        let raydir = (center_f2 - lightpos_f2).normalize();

        let surf = izip!(
            asteroid2.surface.vertices.chunks_exact(3),
            &asteroid2.surface.faces
        )
        //
        // filters facets with distance closer than studied facet.
        .filter(|(_, f)| (f.vertex.position - lightpos_f2).magnitude() < raydist)
        //
        // filters TO BE CONFIRMED
        // .filter(|(_, f)| (f.vertex.position - center_f2).magnitude() > area_sqrt)
        //
        .map(|(vf, _)| (&vf[0].position, &vf[1].position, &vf[2].position))
        .collect();

        if let Some(_) = intersect_surface(&lightpos_f2, &raydir, surf, true) {
            shadowed.push(index);
        }
    }

    shadowed
}
