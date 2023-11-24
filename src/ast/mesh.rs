use crate::{intersect_surface, util::*, vec3_to_4_one, Asteroid, FaceData};

use itertools::{iproduct, izip};

/**
Compute the view factor per unit of area describing the fraction of energy that can be transmitted between the `Face`s
of `Surface` **A** and the `Face`s of `Surface` **B**.

## Definition

The contribution between two `Face`s to the viewing factor depends on the angle between one `Face` normal and the line
of sight between the two `Face`s.

The viewing factor contribution between two `Face`s is zero if:
- the angle between any of the two `Face`s normal and the line of sight between these two is greater than 90 degrees, or
- the line of sight between these two is intercepted by any of the two `Surface`s.

Nalgebra matrices are column-major order. So the filling of the view-factor matrix is column-major order. The cartesian
product of itertools over facets of both surfaces will iterate first over A and then B. To get an output of shape
(N1, N2) we need to transpose the matrix at the end.

The shape of the output matrix is `(N1, N2)`.
There are `N1` rows and `N2` columns.
`N1` is the number of `Face` in `Surface` **A**, and `N2` the number of `Face` in `Surface` **B**.
In this text, i-th indices refers to rows and j-th are for columns.
Which means, each column is a new `Face` of `Surface` **B**, and each row is a new `Face` of `Surface` **A**.
In other words, a row is of shape `(1, N2)` containing one `Face` of `Surface` **A** and all the `Face`s of the
`Surface` **B**, and a column is of shape `(N1, 1)` containing all the `Face`s of `Surface` **B** and one `Face` of
`Surface` **A**.
So if you are looking for the contributions of all `Face`s of `Surface` **B** onto the i-th `Face` of `Surface` **A**,
you need the i-th row.

The only thing is, this function actually returns the view factor per unit of area. To correctly express the fraction of
energy that can be transmitted from facets B to A, each individual facet contribution needs to me multiplied by the area
of the facets transmitting energy. Each column should be multiplied by each facet area from surface B. And to get the
energy transmitted from surface A to surface B, each row should be multiplied by the area of the facets A. We let the
user to do that to that his own way to handle memory as prefered because matrices can be very large and can explode the
RAM.

`Surface` **B** needs to be correctly oriented and positioned in the reference frame of `Surface` **A** to do the
computation of the view factor. Thus, a transformation matrix is needed to express the rotation and translation from
`Surface` **B** to `Surface` **A**.

## Expression

$$V_{ij}=\frac{a_j\cos\Theta_i\cos\Theta_j}{\pi r_{ij}^2}$$

where $a_i$ is the area of the facet $i$, $\Theta_x$ the angle between the centers of the
facets $i$ and $j$ and the normal of the facet $x$, and $r$ is the distance between the
facets.
*/
pub fn view_factor(b1: &Asteroid, b2: &Asteroid, shadows: bool) -> DMatrix<Float> {
    assert!(!b1.surface.is_smooth());
    assert!(!b2.surface.is_smooth());

    // Create a transposed Matrix. The result will be transposed at the return statement. The final dimension is
    // (N1, N2).
    // We do that because `Matrix::iter_mut` is column-major.
    // Another idea could be to create our own `MatrixIter` type but row-major.
    // Or maybe to inverse the transformation matrix to do the calculation correctly in column-major as intended (to be
    // checked, it must be symmetric).
    let mut view_factor = DMatrix::zeros(b2.surface.faces.len(), b1.surface.faces.len());

    let it_faces_b1 = b1.surface.faces.iter();
    let it_faces_b2 = b2.surface.faces.iter();

    let b1_inv_matrix_model = glm::inverse(&b1.matrix_model);
    let inv_m1_m2 = b1_inv_matrix_model * b2.matrix_model;
    let inv_m2_m1 = glm::inverse(&inv_m1_m2);

    // Test if b1 == b2 ==> self view-factor.
    let pos1 = b1.matrix_model.column(3).xyz();
    let pos2 = b2.matrix_model.column(3).xyz();
    let d1 = (pos2 - pos1).magnitude();
    let radius_b1f1 = (b1.surface.faces[0].vertex.position - pos1).magnitude();
    let self_vf = d1 < radius_b1f1;

    for (((index_b1, face_b1), (index_b2, face_b2)), view) in izip!(
        iproduct!(it_faces_b1.enumerate(), it_faces_b2.enumerate()),
        view_factor.iter_mut()
    ) {
        // Vector from center of `Face` **A** to `Face` **B**.
        let center_b2_in_b1 = (inv_m1_m2 * vec3_to_4_one(&face_b2.vertex.position)).xyz();
        let vector_b1_to_b2 = center_b2_in_b1 - face_b1.vertex.position;
        let distance_b1_to_b2 = vector_b1_to_b2.magnitude();
        let unit_b1_to_b2 = vector_b1_to_b2.normalize();

        let _area_sqrt_b1 = face_b1.area.sqrt();
        let area_sqrt_b2 = face_b2.area.sqrt();

        let center_b1_in_b2 = (inv_m2_m1 * vec3_to_4_one(&face_b1.vertex.position)).xyz();
        let vector_b2_to_b1 = center_b1_in_b2 - face_b2.vertex.position;
        let unit_b2_to_b1 = vector_b2_to_b1.normalize();

        // This is a condition on the relation between distance of `Facet`s and their surface area to avoid too large
        // view factor in case of very close distance.
        // About that, a TODO is to subdivide the `Face`s.
        if distance_b1_to_b2 < area_sqrt_b2 {
            continue;
        }

        // Angles from both facet centers using normals and the unit vector pointing to the other `Face`.
        let angle_at_b1 = face_b1.vertex.normal.angle(&unit_b1_to_b2);
        let angle_at_b2 = face_b2.vertex.normal.angle(&unit_b2_to_b1);

        // Another condition is one that was actually mentioned earlier: angles must be smaller than 90°.
        if angle_at_b1 >= PI / 2.0 || angle_at_b2 >= PI / 2.0 {
            continue;
        }

        if shadows {
            if !self_vf {
                // Surface of body B intercepting ray from facet A to facet B.
                let surfv = izip!(b2.surface.vertices.chunks_exact(3), &b2.surface.faces)
                    .enumerate()
                    //
                    // filters other facets than facet_b.
                    .filter(|(index, (_, _))| index_b2 != *index)
                    //
                    // filters facets with distance closer than facet_b with margin using area.
                    .filter(|(_index, (_, f))| {
                        (f.vertex.position - center_b1_in_b2).magnitude() < distance_b1_to_b2
                    })
                    //
                    .map(|(_, (vf, _))| (&vf[0].position, &vf[1].position, &vf[2].position))
                    .collect();

                if let Some(_) = intersect_surface(&center_b1_in_b2, &unit_b2_to_b1, surfv, true) {
                    continue;
                }
            }

            // Surface of body A intercepting ray from facet A to facet B.
            let surfv = izip!(b1.surface.vertices.chunks_exact(3), &b1.surface.faces)
                .enumerate()
                //
                // filters other facets than facet_a.
                // and for self-view-factor only -> filters other facets than facet_b.
                .filter(|(index, (_, _))| index_b1 != *index && (!self_vf || index_b2 != *index))
                //
                // filters facets with distance closer than facet_a with margin using area.
                .filter(|(_index, (_, f))| {
                    (f.vertex.position - center_b2_in_b1).magnitude() < distance_b1_to_b2
                })
                //
                .map(|(_, (vf, _))| (&vf[0].position, &vf[1].position, &vf[2].position))
                .collect();

            if let Some(_) =
                intersect_surface(&face_b1.vertex.position, &unit_b1_to_b2, surfv, true)
            {
                continue;
            }
        }

        // Well, ready for calculation.
        // *view = view_factor_scalar(face_b.area, angle_at_a, angle_at_b, distance_a2b)
        *view = view_factor_scalar_by_unit_of_area(angle_at_b1, angle_at_b2, distance_b1_to_b2)
    }
    view_factor.transpose()
}

/**
Compute the view factor between two `Face`s. `transformation_b2a` is applied to `face_b` to position and orient it in
fixed-frame of `face_a`.
*/
pub fn view_factor_face(face_a: &FaceData, face_b: &FaceData, transformation_b2a: &Mat4) -> Float {
    // Vector from center of `Face` **A** to `Face` **B**.
    let vector_a2b = (transformation_b2a * vec3_to_4_one(&face_b.vertex.position)).xyz()
        - face_a.vertex.position;
    let distance_a2b = vector_a2b.magnitude();
    let unit_a2b = vector_a2b.normalize();

    // This is a condition on the relation between distance of `Facet`s and their surface area to avoid too large
    // view factor in case of very close distance.
    // About that, a TODO is to subdivide the `Face`s.
    if distance_a2b < face_b.area.sqrt() {
        return 0.0;
    }

    // Angles from both normals and the unit vector to the other `Face`.
    // The normal of `Face` **B** needs to be transformed to fixed-frame **A** for correct calculation.
    let angle_at_a = face_a.vertex.normal.angle(&unit_a2b);
    let angle_at_b = transformation_b2a
        .transform_vector(&face_b.vertex.normal)
        .angle(&-unit_a2b);

    // Another condition is one that was actually mentioned earlier: angles must be smaller than 90°.
    if angle_at_a >= PI / 2.0 || angle_at_b >= PI / 2.0 {
        return 0.0;
    }

    // Well, ready for calculation.
    view_factor_scalar(face_b.area, angle_at_a, angle_at_b, distance_a2b)
}

/**
Compute the view factor between a `Face` A and a `Face` B from the variables of the equation.
See `view_factor()` for more info.
*/
pub fn view_factor_scalar(
    area_b: Float,
    angle_at_a: Float,
    angle_at_b: Float,
    distance_a2b: Float,
) -> Float {
    area_b * angle_at_a.cos() * angle_at_b.cos() / (PI * distance_a2b.powi(2))
}

pub fn view_factor_scalar_by_unit_of_area(
    angle_at_a: Float,
    angle_at_b: Float,
    distance_a2b: Float,
) -> Float {
    angle_at_a.cos() * angle_at_b.cos() / (PI * distance_a2b.powi(2))
}
