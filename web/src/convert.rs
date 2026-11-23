//! What the page's Python hands kalast, as kalast's types, and what kalast
//! hands back: the conversions the desktop's bindings get from pyo3, on
//! JavaScript's values. A Python number, string, bool or None arrives as
//! JavaScript's own; a list or a numpy array as a JavaScript array or a
//! typed array (`web/python/kalast/_rs/_page.py` sends them so).
//!
//! An error says, before its colon, what Python raises: `ValueError: ...`.
//! The page's `kalast._rs` raises that.

use wasm_bindgen::{JsCast, JsValue};

/// Python's `None`, which arrives as `undefined` -- and goes back as it:
/// Pyodide makes JavaScript's `null` a `jsnull`, which is not `None`.
pub fn absent(v: &JsValue) -> bool {
    v.is_null() || v.is_undefined()
}

fn kind(v: &JsValue) -> String {
    if absent(v) { "None".to_string() } else { v.js_typeof().as_string().unwrap_or_default() }
}

pub fn number(v: &JsValue, name: &str) -> Result<f64, String> {
    v.as_f64().ok_or_else(|| format!("TypeError: {name} must be a number, not {}", kind(v)))
}

pub fn boolean(v: &JsValue, name: &str) -> Result<bool, String> {
    v.as_bool().ok_or_else(|| format!("TypeError: {name} must be True or False, not {}", kind(v)))
}

pub fn integer(v: &JsValue, name: &str) -> Result<i64, String> {
    let x = number(v, name)?;
    if x.fract() != 0.0 || !x.is_finite() {
        return Err(format!("TypeError: {name} must be an integer, not {x}"));
    }
    Ok(x as i64)
}

pub fn unsigned(v: &JsValue, name: &str) -> Result<u64, String> {
    let x = integer(v, name)?;
    u64::try_from(x).map_err(|_| format!("OverflowError: {name} cannot be negative, got {x}"))
}

pub fn string(v: &JsValue, name: &str) -> Result<String, String> {
    v.as_string().ok_or_else(|| format!("TypeError: {name} must be a string, not {}", kind(v)))
}

/// Numbers from a JavaScript array or a typed array.
pub fn float_vec(v: &JsValue, name: &str) -> Result<Vec<f64>, String> {
    if let Some(a) = v.dyn_ref::<js_sys::Float64Array>() {
        return Ok(a.to_vec());
    }
    if let Some(a) = v.dyn_ref::<js_sys::Float32Array>() {
        return Ok(a.to_vec().into_iter().map(f64::from).collect());
    }
    if js_sys::Array::is_array(v) {
        let a: &js_sys::Array = v.unchecked_ref();
        return a.iter().map(|x| number(&x, name)).collect();
    }
    if let Some(x) = v.as_f64() {
        return Ok(vec![x]);
    }
    Err(format!("TypeError: {name} must be a sequence of numbers, not {}", kind(v)))
}

/// Exactly `N` numbers.
pub fn floats<const N: usize>(v: &JsValue, name: &str) -> Result<[f64; N], String> {
    let all = float_vec(v, name)?;
    <[f64; N]>::try_from(all.as_slice())
        .map_err(|_| format!("ValueError: {name} takes {N} numbers, got {}", all.len()))
}

/// Unsigned integers from a JavaScript array or a typed array.
pub fn index_vec(v: &JsValue, name: &str) -> Result<Vec<usize>, String> {
    float_vec(v, name)?
        .into_iter()
        .map(|x| {
            if x.fract() == 0.0 && x >= 0.0 {
                Ok(x as usize)
            } else {
                Err(format!("TypeError: {name} must hold indices, not {x}"))
            }
        })
        .collect()
}

/// A short list of numbers, as Python's list comes back.
pub fn floats_value(values: &[f64]) -> JsValue {
    values.iter().map(|&x| JsValue::from_f64(x)).collect::<js_sys::Array>().into()
}

/// A tuple or list of values.
pub fn list(values: impl IntoIterator<Item = JsValue>) -> JsValue {
    values.into_iter().collect::<js_sys::Array>().into()
}

/// A numpy array for Python: its values, their type and its shape --
/// `{t: "a", d, s, k}`, `k` set when Python's array writes back to what it
/// came from (`_page.py`'s `_Linked`), as the desktop's views onto kalast's
/// own memory do.
pub fn array(data: JsValue, shape: &[usize], linked: bool) -> JsValue {
    let o = js_sys::Object::new();
    let set = |k: &str, v: &JsValue| {
        let _ = js_sys::Reflect::set(&o, &JsValue::from_str(k), v);
    };
    set("t", &JsValue::from_str("a"));
    set("d", &data);
    set("s", &shape.iter().map(|&n| JsValue::from_f64(n as f64)).collect::<js_sys::Array>());
    set("k", &JsValue::from_bool(linked));
    o.into()
}

pub fn array_f32(values: &[f32], shape: &[usize], linked: bool) -> JsValue {
    array(js_sys::Float32Array::from(values).into(), shape, linked)
}

pub fn array_f64(values: &[f64], shape: &[usize], linked: bool) -> JsValue {
    array(js_sys::Float64Array::from(values).into(), shape, linked)
}

pub fn array_u32(values: &[u32], shape: &[usize]) -> JsValue {
    array(js_sys::Uint32Array::from(values).into(), shape, false)
}

/// Indices, which numpy is given as `int64`, as the desktop's bindings give
/// them: `{..., y: "int64"}` has `_page.py` cast them.
pub fn array_i64(values: &[usize], shape: &[usize]) -> JsValue {
    let as_u32: Vec<u32> = values.iter().map(|&i| i as u32).collect();
    let a = array(js_sys::Uint32Array::from(as_u32.as_slice()).into(), shape, false);
    let _ = js_sys::Reflect::set(&a, &JsValue::from_str("y"), &JsValue::from_str("int64"));
    a
}

/// One of kalast's objects for Python, by its kind and index: `{t: "o", k,
/// i}`, which `_page.py` makes the class of that name.
pub fn object(kind: &str, id: usize) -> JsValue {
    let o = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&o, &JsValue::from_str("t"), &JsValue::from_str("o"));
    let _ = js_sys::Reflect::set(&o, &JsValue::from_str("k"), &JsValue::from_str(kind));
    let _ = js_sys::Reflect::set(&o, &JsValue::from_str("i"), &JsValue::from_f64(id as f64));
    o.into()
}

/// A method of an object, for Python to call: `{t: "m"}`.
pub fn method() -> JsValue {
    let o = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&o, &JsValue::from_str("t"), &JsValue::from_str("m"));
    o.into()
}

/// The `n`th argument of a call, `None` past the end.
pub fn arg(args: &js_sys::Array, n: u32) -> JsValue {
    args.get(n)
}
