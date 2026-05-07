use std::sync::Mutex;

use once_cell::sync::Lazy;
use pyo3::prelude::*;

pub fn isize_to_usize(mut index: isize, n: usize) -> PyResult<usize> {
    if index < 0 {
        index += n as isize;
    }
    let index = index as usize;
    if index >= n {
        return Err(pyo3::exceptions::PyIndexError::new_err("out of bounds"));
    }
    Ok(index)
}

#[macro_export]
macro_rules! pyadd_c {
    ($m:expr, $p:ident $(::$c:tt)+) => {
        // 1) receive `pyadd_c!(crate::tpm::core::NEWTON_METHOD_MAX_ITERATION);`
        // split $p and send to 2)
        pyadd_c!($m, $p::, $($c),*);
    };

    ($m:expr, $($p:ident::)+, $head:ident, $($tail:ident),+) => {
        // 2) receive $p + a split version of it.
        // The first time it isolate the head `tpm` and call 2) again.
        // The second time it isolate the new head `core` and only one tail so it will call 3).
        pyadd_c!($m, $($p::)* $head::, $($tail),*);
    };

    ($m:expr, $($p:ident::)+ , $c:ident) => {
        // 3) this is called when $method is NEWTON_METHOD_MAX_ITERATION and $module is the full
        // path before.
        $m.add(stringify!($c), $($p::)+ $c)?;
    };
}

#[macro_export]
macro_rules! pyadd_f {
    ($m:expr, $f:path) => {
        $m.add_function(wrap_pyfunction!($f, &$m)?)?;
    };
}

pub fn pyadd_c_lazy<'py, T>(m: &Bound<'py, PyModule>, name: &str, lazy_data: &Lazy<Mutex<T>>)
where
    T: Clone + std::fmt::Debug + IntoPyObject<'py>,
{
    let locked = lazy_data.lock().unwrap();
    m.add(name, locked.clone()).unwrap();
}

#[macro_export]
macro_rules! impl_py_attrs_vec {
    ($ty:ident, $( $field:ident : $len:expr ),+ $(,)?) => {
        paste::paste!{
            #[pymethods]
            impl $ty {
                $(
                    #[getter]
                    fn $field<'py>(slf: pyo3::Bound<'py, Self>) -> pyo3::Bound<'py, numpy::PyArray1<Float>> {
                        let inner = &slf.borrow().inner;
                        let slice = &inner.borrow().$field;
                        let arr = ndarray::ArrayView1::from(slice.as_ref());
                        unsafe { numpy::PyArray1::borrow_from_array(&arr, slf.into_any()) }
                    }

                    #[setter]
                    fn [<set_ $field>](&self, arr: [Float; $len]) {
                        self.inner.borrow_mut().$field = arr.into();
                    }
                )+
            }
        }
    };
}

#[macro_export]
macro_rules! impl_mesh_view {
    ($collection_name:ident, $view_name:ident, $wrapper_name:ident, $field:ident) => {
        #[pyclass(unsendable)]
        pub struct $collection_name {
            mesh: Rc<RefCell<crate::mesh::Mesh>>,
        }

        #[pymethods]
        impl $collection_name {
            fn __len__(&self) -> usize {
                self.mesh.borrow().$field.len()
            }

            fn __getitem__(&self, index: isize) -> PyResult<$view_name> {
                let index = super::util::isize_to_usize(index, self.mesh.borrow().$field.len())?;

                Ok($view_name {
                    mesh: self.mesh.clone(),
                    index,
                })
            }

            pub fn append(&mut self, element: $wrapper_name) {
                let mut mesh = self.mesh.borrow_mut();
                mesh.$field.push(element.inner.borrow().clone());
            }

            pub fn clear(&mut self) {
                self.mesh.borrow_mut().$field.clear();
            }

            pub fn extend(&mut self, elements: Vec<$wrapper_name>) {
                let mut mesh = self.mesh.borrow_mut();
                mesh.$field
                    .extend(elements.into_iter().map(|e| e.inner.borrow().clone()));
            }

            pub fn __repr__(&self) -> String {
                format!("{:?}", self.mesh.borrow().$field)
            }
        }

        #[pyclass(unsendable)]
        pub struct $view_name {
            pub mesh: Rc<RefCell<crate::mesh::Mesh>>,
            pub index: usize,
        }

        #[pymethods]
        impl $view_name {
            pub fn __repr__(&self) -> String {
                let mesh = self.mesh.borrow();
                format!("{:?}", mesh.$field[self.index])
            }
        }
    };
}

#[macro_export]
macro_rules! impl_mesh_field_vec {
    ($collection_view:ident, $collection:ident, $field:ident) => {
        #[pymethods]
        impl $collection_view {
            #[getter]
            fn $field<'py>(slf: Bound<'py, Self>) -> Bound<'py, numpy::PyArray1<Float>> {
                let this = slf.borrow();
                let mesh = this.mesh.borrow();
                let element = &mesh.$collection[this.index];
                let arr = ndarray::ArrayView1::from(element.$field.as_ref());
                unsafe { numpy::PyArray1::borrow_from_array(&arr, slf.into_any()) }
            }
        }
    };
}

#[macro_export]
macro_rules! impl_mesh_field_scalar {
    ($collection_view:ident, $collection:ident, $field:ident, $ty:ty) => {
        paste::paste! {
            #[pymethods]
            impl $collection_view {
                #[getter]
                fn $field(&self) -> $ty {
                    self.mesh.borrow().$collection[self.index].$field
                }

                #[setter]
                fn [<set_ $field>](&mut self, $field: $ty) {
                    self.mesh.borrow_mut().$collection[self.index].$field = $field;
                }
            }
        }
    };
}
