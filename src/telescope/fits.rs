//! FITS files of one image, as a telescope's camera writes them.
//!
//! The primary header-data unit only, two axes: what a CCD frame is. Written
//! as unsigned 16-bit counts (`BITPIX = 16`, `BZERO = 32768`) the way
//! cameras store ADU, or as 32-bit floats. Read back from any `BITPIX`, with
//! `BSCALE` and `BZERO` applied, so the photometry reads a real night's
//! frames as well as kalast's. No extensions, no compression, no tables.
//!
//! The rows are as the file has them: row 0 of the data is FITS's row 1,
//! the image's bottom as a viewer shows it -- astropy's convention, so a
//! frame read here and one read there index alike.

use std::fs::File;
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::path::Path;

const BLOCK: usize = 2880;
const CARD: usize = 80;

/// A header card's value.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
}

impl From<bool> for Value {
    fn from(v: bool) -> Self {
        Value::Bool(v)
    }
}
impl From<i64> for Value {
    fn from(v: i64) -> Self {
        Value::Int(v)
    }
}
impl From<f64> for Value {
    fn from(v: f64) -> Self {
        Value::Float(v)
    }
}
impl From<&str> for Value {
    fn from(v: &str) -> Self {
        Value::Str(v.to_string())
    }
}
impl From<String> for Value {
    fn from(v: String) -> Self {
        Value::Str(v)
    }
}

impl Value {
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Int(v) => Some(*v as f64),
            Value::Float(v) => Some(*v),
            _ => None,
        }
    }
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }
}

/// A header: its cards in order, the structural ones (`SIMPLE`, `BITPIX`,
/// `NAXIS*`, `BZERO`, `BSCALE`, `END`) left out -- `write` makes them.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Header {
    pub cards: Vec<(String, Value, String)>,
}

impl Header {
    /// Set `key`, replacing a card of that name or adding one at the end.
    pub fn set(&mut self, key: &str, value: impl Into<Value>, comment: &str) {
        let key = key.to_ascii_uppercase();
        let value = value.into();
        match self.cards.iter_mut().find(|(k, _, _)| *k == key) {
            Some(card) => {
                card.1 = value;
                card.2 = comment.to_string();
            }
            None => self.cards.push((key, value, comment.to_string())),
        }
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        let key = key.to_ascii_uppercase();
        self.cards.iter().find(|(k, _, _)| *k == key).map(|(_, v, _)| v)
    }

    pub fn f64(&self, key: &str) -> Option<f64> {
        self.get(key).and_then(Value::as_f64)
    }

    pub fn str(&self, key: &str) -> Option<&str> {
        self.get(key).and_then(Value::as_str)
    }
}

/// How the data are stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Storage {
    /// Unsigned 16-bit counts, rounded and clipped to 0..=65535: a camera's.
    U16,
    /// 32-bit floats, as given.
    F32,
}

/// An image and its header.
#[derive(Debug, Clone)]
pub struct Image {
    pub width: usize,
    pub height: usize,
    /// Row-major, `width` values a row, row 0 FITS's row 1.
    pub data: Vec<f32>,
    pub header: Header,
}

fn card(text: &str) -> [u8; CARD] {
    let mut out = [b' '; CARD];
    for (o, c) in out.iter_mut().zip(text.bytes().filter(|b| b.is_ascii() && !b.is_ascii_control())) {
        *o = c;
    }
    out
}

/// A float as FITS's fixed format wants it: within 20 characters, with a
/// decimal point or an exponent, never Rust's `inf`.
fn float_text(v: f64) -> String {
    if !v.is_finite() {
        return "0.0".into();
    }
    let a = v.abs();
    if a != 0.0 && !(1e-4..1e15).contains(&a) {
        return format!("{v:.12E}");
    }
    let s = format!("{v}");
    let s = if s.contains('.') { s } else { format!("{s}.0") };
    if s.len() > 20 {
        format!("{v:.12E}")
    } else {
        s
    }
}

fn value_card(key: &str, value: &Value, comment: &str) -> [u8; CARD] {
    let field = match value {
        Value::Bool(b) => format!("{:>20}", if *b { "T" } else { "F" }),
        Value::Int(i) => format!("{i:>20}"),
        Value::Float(f) => format!("{:>20}", float_text(*f)),
        Value::Str(s) => {
            let quoted = s.replace('\'', "''");
            format!("'{quoted:<8}'")
        }
    };
    let mut text = format!("{:<8}= {field}", key);
    if !comment.is_empty() {
        text.push_str(" / ");
        text.push_str(comment);
    }
    card(&text)
}

/// Write `data`, `width` x `height`, with `header`'s cards after the
/// structural ones.
pub fn write(path: &Path, width: usize, height: usize, data: &[f32], header: &Header, storage: Storage) -> io::Result<()> {
    if data.len() != width * height {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{} values for a {width} x {height} image", data.len()),
        ));
    }
    let mut cards: Vec<[u8; CARD]> = Vec::new();
    cards.push(value_card("SIMPLE", &Value::Bool(true), "conforms to FITS"));
    let bitpix = match storage {
        Storage::U16 => 16,
        Storage::F32 => -32,
    };
    cards.push(value_card("BITPIX", &Value::Int(bitpix), ""));
    cards.push(value_card("NAXIS", &Value::Int(2), ""));
    cards.push(value_card("NAXIS1", &Value::Int(width as i64), ""));
    cards.push(value_card("NAXIS2", &Value::Int(height as i64), ""));
    if storage == Storage::U16 {
        cards.push(value_card("BZERO", &Value::Float(32768.0), "unsigned 16-bit counts"));
        cards.push(value_card("BSCALE", &Value::Float(1.0), ""));
    }
    for (key, value, comment) in &header.cards {
        let key = key.to_ascii_uppercase();
        if matches!(key.as_str(), "SIMPLE" | "BITPIX" | "NAXIS" | "NAXIS1" | "NAXIS2" | "BZERO" | "BSCALE" | "END" | "EXTEND") {
            continue;
        }
        cards.push(value_card(&key, value, comment));
    }
    cards.push(card("END"));

    let mut out = BufWriter::new(File::create(path)?);
    let mut written = 0;
    for c in &cards {
        out.write_all(c)?;
        written += CARD;
    }
    out.write_all(&vec![b' '; (BLOCK - written % BLOCK) % BLOCK])?;

    let mut bytes = Vec::with_capacity(data.len() * if storage == Storage::U16 { 2 } else { 4 });
    match storage {
        Storage::U16 => {
            for &v in data {
                let adu = if v.is_finite() { v.round().clamp(0.0, 65535.0) } else { 0.0 } as i32;
                bytes.extend_from_slice(&((adu - 32768) as i16).to_be_bytes());
            }
        }
        Storage::F32 => {
            for &v in data {
                bytes.extend_from_slice(&v.to_be_bytes());
            }
        }
    }
    out.write_all(&bytes)?;
    out.write_all(&vec![0u8; (BLOCK - bytes.len() % BLOCK) % BLOCK])?;
    out.flush()
}

fn parse_value(field: &str) -> Option<Value> {
    let field = field.trim_start();
    if let Some(rest) = field.strip_prefix('\'') {
        // Up to the closing quote, a doubled one standing for itself.
        let mut s = String::new();
        let mut chars = rest.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '\'' {
                if chars.peek() == Some(&'\'') {
                    chars.next();
                    s.push('\'');
                } else {
                    break;
                }
            } else {
                s.push(c);
            }
        }
        return Some(Value::Str(s.trim_end().to_string()));
    }
    let text = field.split('/').next().unwrap_or("").trim();
    match text {
        "" => None,
        "T" => Some(Value::Bool(true)),
        "F" => Some(Value::Bool(false)),
        _ => text
            .parse::<i64>()
            .map(Value::Int)
            .ok()
            .or_else(|| text.replace(['D', 'd'], "E").parse::<f64>().map(Value::Float).ok()),
    }
}

/// Read a FITS file's primary image.
pub fn read(path: &Path) -> io::Result<Image> {
    let bad = |m: String| io::Error::new(io::ErrorKind::InvalidData, format!("{}: {m}", path.display()));
    let mut input = BufReader::new(File::open(path)?);
    let mut all = Header::default();
    let mut block = vec![0u8; BLOCK];
    'header: loop {
        input.read_exact(&mut block).map_err(|_| bad("no END card".into()))?;
        for raw in block.chunks(CARD) {
            let text = String::from_utf8_lossy(raw);
            let key = text.get(..8).unwrap_or("").trim().to_string();
            if key == "END" {
                break 'header;
            }
            if text.get(8..10) == Some("= ") {
                if let Some(value) = parse_value(&text[10..]) {
                    all.cards.push((key, value, String::new()));
                }
            }
        }
    }
    let int = |key: &str| all.f64(key).map(|v| v as i64).ok_or_else(|| bad(format!("no {key}")));
    let bitpix = int("BITPIX")?;
    let naxis = int("NAXIS")?;
    if naxis != 2 {
        return Err(bad(format!("NAXIS = {naxis}: an image of two axes is read")));
    }
    if ![8, 16, 32, 64, -32, -64].contains(&bitpix) {
        return Err(bad(format!("BITPIX = {bitpix}")));
    }
    let (width, height) = (int("NAXIS1")? as usize, int("NAXIS2")? as usize);
    let zero = all.f64("BZERO").unwrap_or(0.0);
    let scale = all.f64("BSCALE").unwrap_or(1.0);
    let size = (bitpix.unsigned_abs() / 8) as usize;
    let mut bytes = vec![0u8; width * height * size];
    input.read_exact(&mut bytes).map_err(|_| bad("the data are short".into()))?;
    let data: Vec<f32> = bytes
        .chunks_exact(size)
        .map(|b| {
            let raw = match bitpix {
                8 => b[0] as f64,
                16 => i16::from_be_bytes([b[0], b[1]]) as f64,
                32 => i32::from_be_bytes(b.try_into().unwrap()) as f64,
                64 => i64::from_be_bytes(b.try_into().unwrap()) as f64,
                -32 => f32::from_be_bytes(b.try_into().unwrap()) as f64,
                -64 => f64::from_be_bytes(b.try_into().unwrap()),
                _ => f64::NAN,
            };
            (zero + scale * raw) as f32
        })
        .collect();
    let structural = ["SIMPLE", "BITPIX", "NAXIS", "NAXIS1", "NAXIS2", "BZERO", "BSCALE", "EXTEND"];
    let header = Header {
        cards: all.cards.into_iter().filter(|(k, _, _)| !structural.contains(&k.as_str())).collect(),
    };
    Ok(Image { width, height, data, header })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("kalast-fits-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join(name)
    }

    #[test]
    fn a_frame_comes_back_as_written() {
        let (w, h) = (37, 23);
        let data: Vec<f32> = (0..w * h).map(|i| (i * 97 % 65536) as f32).collect();
        let mut header = Header::default();
        header.set("EXPTIME", 120.0, "seconds");
        header.set("FILTER", "Rc", "");
        header.set("OBJECT", "Didymos's 'system'", "quotes doubled");
        header.set("NFRAME", 7i64, "");
        header.set("TINY", 1.25e-9, "");
        header.set("TRACKED", true, "");
        let path = scratch("u16.fits");
        write(&path, w, h, &data, &header, Storage::U16).unwrap();
        assert_eq!(std::fs::metadata(&path).unwrap().len() % BLOCK as u64, 0);
        let back = read(&path).unwrap();
        assert_eq!((back.width, back.height), (w, h));
        assert_eq!(back.data, data);
        assert_eq!(back.header.f64("EXPTIME"), Some(120.0));
        assert_eq!(back.header.str("FILTER"), Some("Rc"));
        assert_eq!(back.header.str("OBJECT"), Some("Didymos's 'system'"));
        assert_eq!(back.header.get("NFRAME"), Some(&Value::Int(7)));
        assert_eq!(back.header.f64("TINY"), Some(1.25e-9));
        assert_eq!(back.header.get("TRACKED"), Some(&Value::Bool(true)));
    }

    #[test]
    fn counts_are_rounded_and_clipped_floats_kept() {
        let data = [-5.0, 0.4, 0.6, 65535.4, 70000.0, f32::NAN];
        let path = scratch("clip.fits");
        write(&path, 3, 2, &data, &Header::default(), Storage::U16).unwrap();
        assert_eq!(read(&path).unwrap().data, vec![0.0, 0.0, 1.0, 65535.0, 65535.0, 0.0]);
        let data = [-5.25f32, 0.1, 1e-30, 3.5e7];
        write(&path, 2, 2, &data, &Header::default(), Storage::F32).unwrap();
        assert_eq!(read(&path).unwrap().data, data.to_vec());
    }

    #[test]
    fn cards_follow_the_fixed_format() {
        let c = value_card("EXPTIME", &Value::Float(120.0), "s");
        let text = std::str::from_utf8(&c).unwrap();
        assert_eq!(&text[..10], "EXPTIME = ");
        assert_eq!(text[10..30].trim_start(), "120.0");
        assert_eq!(&text[29..30], "0", "right-justified to column 30");
        let c = value_card("FILTER", &Value::Str("Rc".into()), "");
        assert_eq!(&std::str::from_utf8(&c).unwrap()[10..20], "'Rc      '");
        assert_eq!(float_text(1.0e-9), "1.000000000000E-9");
    }
}
