use ndarray::{Array1, ArrayView1, s};

use crate::Float;

pub fn update_thermal_state(
    t: ArrayView1<'_, Float>,
    f: Float,
    d: ArrayView1<'_, Float>,
    dtpdx2: ArrayView1<'_, Float>,
    se: Float,
    k: Float,
    twodx: Float,
) -> Array1<Float> {
    let n = t.len();
    let mut new_t = t.to_owned();
    new_t[0] = super::core::newton_method(t[0], f, se, k, t[1], t[2], twodx).unwrap();
    let new_t_in = super::core::conduction_1d(new_t.view(), d, dtpdx2);
    new_t.slice_mut(s![1..-1]).assign(&new_t_in);
    new_t[n - 1] = new_t[n - 2];
    new_t
}

#[cfg(feature = "python")]
pub(crate) mod py {
    use numpy::{PyArray1, PyReadonlyArray1, ToPyArray};
    #[cfg(feature = "python")]
use pyo3::prelude::*;

    use crate::Float;

    #[cfg_attr(feature = "python", pyfunction)]
    pub fn update_thermal_state<'py>(
        py: Python<'py>,
        t: PyReadonlyArray1<'py, Float>,
        f: Float,
        d: PyReadonlyArray1<'py, Float>,
        dtpdx2: PyReadonlyArray1<'py, Float>,
        se: Float,
        k: Float,
        twodx: Float,
    ) -> Bound<'py, PyArray1<Float>> {
        super::update_thermal_state(
            t.as_array(),
            f,
            d.as_array(),
            dtpdx2.as_array(),
            se,
            k,
            twodx,
        )
        .to_pyarray(py)
    }
}
