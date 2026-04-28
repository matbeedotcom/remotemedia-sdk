//! Minimal NumPy `.npy` v1/v2 reader.
//!
//! Rust port of [`external/.../Utils/IO/NpyReader.cs`](../../../../../../external/handcrafted-persona-engine/src/PersonaEngine/PersonaEngine.Lib/Utils/IO/NpyReader.cs).
//!
//! Supports float32 (`<f4`) and int32 (`<i4`) C-contiguous (row-major)
//! arrays — that's what the persona-engine bundle's NPZs ship and is
//! what every consumer in this codebase needs. Other dtypes / endianness
//! are rejected with a clear error.
//!
//! Reads from any `Read`er — the [`super::npz::NpzArchive`] wraps
//! `zip::read::ZipFile` streams here.
//!
//! Format reference: <https://numpy.org/doc/stable/reference/generated/numpy.lib.format.html>

use std::io::Read;

/// `\x93NUMPY` — fixed magic-bytes prefix on every `.npy` file.
const MAGIC: [u8; 6] = [0x93, b'N', b'U', b'M', b'P', b'Y'];

/// Loaded f32 data + its row-major shape.
#[derive(Debug)]
pub struct NpyF32 {
    pub data: Vec<f32>,
    pub shape: Vec<usize>,
}

/// Loaded i32 data + its row-major shape.
#[derive(Debug)]
pub struct NpyI32 {
    pub data: Vec<i32>,
    pub shape: Vec<usize>,
}

/// Read a `.npy` stream as float32. Errors on non-`<f4` dtype or
/// truncated payloads.
pub fn read_f32<R: Read>(mut reader: R, source: &str) -> Result<NpyF32, NpyError> {
    let (dtype, shape) = read_header(&mut reader, source)?;
    if dtype != "<f4" && dtype != "f4" {
        return Err(NpyError::WrongDtype {
            expected: "<f4",
            actual: dtype,
            context: source.to_string(),
        });
    }
    let total = shape_total(&shape);
    let mut data = vec![0.0f32; total];
    let bytes = bytemuck::cast_slice_mut(&mut data[..]);
    reader
        .read_exact(bytes)
        .map_err(|e| NpyError::Io(e, source.to_string()))?;
    Ok(NpyF32 { data, shape })
}

/// Read a `.npy` stream as int32.
pub fn read_i32<R: Read>(mut reader: R, source: &str) -> Result<NpyI32, NpyError> {
    let (dtype, shape) = read_header(&mut reader, source)?;
    if dtype != "<i4" && dtype != "i4" {
        return Err(NpyError::WrongDtype {
            expected: "<i4",
            actual: dtype,
            context: source.to_string(),
        });
    }
    let total = shape_total(&shape);
    let mut data = vec![0i32; total];
    let bytes = bytemuck::cast_slice_mut(&mut data[..]);
    reader
        .read_exact(bytes)
        .map_err(|e| NpyError::Io(e, source.to_string()))?;
    Ok(NpyI32 { data, shape })
}

/// Errors surfaced by the NPY reader.
#[derive(Debug, thiserror::Error)]
pub enum NpyError {
    #[error("not a valid .npy file (bad magic) in {0}")]
    BadMagic(String),

    #[error("unsupported .npy version {version} in {context}")]
    UnsupportedVersion { version: u8, context: String },

    #[error("wrong dtype in {context}: expected {expected}, got {actual}")]
    WrongDtype {
        expected: &'static str,
        actual: String,
        context: String,
    },

    #[error("malformed header in {context}: {message}")]
    BadHeader { context: String, message: String },

    #[error("invalid shape in {context}: {message}")]
    BadShape { context: String, message: String },

    #[error("io error reading {1}: {0}")]
    Io(#[source] std::io::Error, String),
}

/// Compute total element count from a shape vector. Empty shape → 1
/// (matches NumPy zero-d scalar semantics).
fn shape_total(shape: &[usize]) -> usize {
    shape.iter().copied().product::<usize>().max(1)
}

/// Parse the `.npy` preamble + header, returning `(dtype, shape)`.
fn read_header<R: Read>(
    reader: &mut R,
    source: &str,
) -> Result<(String, Vec<usize>), NpyError> {
    // Preamble: 10 bytes for v1, 12 for v2. We always read 10 first.
    let mut preamble = [0u8; 10];
    reader
        .read_exact(&mut preamble)
        .map_err(|e| NpyError::Io(e, source.to_string()))?;
    if preamble[..6] != MAGIC {
        return Err(NpyError::BadMagic(source.to_string()));
    }
    let major = preamble[6];
    let header_len = match major {
        1 => u16::from_le_bytes([preamble[8], preamble[9]]) as usize,
        2 => {
            let mut tail = [0u8; 2];
            reader
                .read_exact(&mut tail)
                .map_err(|e| NpyError::Io(e, source.to_string()))?;
            u32::from_le_bytes([preamble[8], preamble[9], tail[0], tail[1]]) as usize
        }
        v => {
            return Err(NpyError::UnsupportedVersion {
                version: v,
                context: source.to_string(),
            })
        }
    };
    let mut header_buf = vec![0u8; header_len];
    reader
        .read_exact(&mut header_buf)
        .map_err(|e| NpyError::Io(e, source.to_string()))?;
    let header = std::str::from_utf8(&header_buf).map_err(|_| NpyError::BadHeader {
        context: source.to_string(),
        message: "header bytes are not valid UTF-8".into(),
    })?;
    parse_header(header, source)
}

/// Extract `dtype` and `shape` from a numpy header dict-string like
/// `{'descr': '<f4', 'fortran_order': False, 'shape': (768, 1024), }`.
fn parse_header(header: &str, source: &str) -> Result<(String, Vec<usize>), NpyError> {
    let dtype = extract_string_value(header, "'descr':").ok_or_else(|| NpyError::BadHeader {
        context: source.to_string(),
        message: "missing 'descr' key".into(),
    })?;
    let shape_str = extract_tuple_value(header, "'shape':").ok_or_else(|| NpyError::BadHeader {
        context: source.to_string(),
        message: "missing 'shape' key".into(),
    })?;
    let inner = shape_str.trim_matches(|c: char| c == '(' || c == ')' || c.is_whitespace());
    let shape: Vec<usize> = if inner.is_empty() {
        vec![1]
    } else {
        inner
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| {
                s.parse::<usize>().map_err(|_| NpyError::BadShape {
                    context: source.to_string(),
                    message: format!("non-integer dim '{s}'"),
                })
            })
            .collect::<Result<Vec<_>, _>>()?
    };
    Ok((dtype, shape))
}

/// Extract a quoted-string value following `key` in the header.
fn extract_string_value(header: &str, key: &str) -> Option<String> {
    let idx = header.find(key)?;
    let rest = &header[idx + key.len()..];
    let rest = rest.trim_start();
    if !rest.starts_with('\'') {
        return None;
    }
    let after_quote = &rest[1..];
    let end = after_quote.find('\'')?;
    Some(after_quote[..end].to_string())
}

/// Extract a tuple `(…, …)` value following `key`. Returns the raw
/// substring including parens.
fn extract_tuple_value(header: &str, key: &str) -> Option<String> {
    let idx = header.find(key)?;
    let rest = &header[idx + key.len()..];
    let rest = rest.trim_start();
    if !rest.starts_with('(') {
        return None;
    }
    let end = rest.find(')')?;
    Some(rest[..=end].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    /// Build a minimal valid v1 .npy byte stream from a dtype +
    /// row-major shape + payload bytes.
    fn synth_npy(dtype: &str, shape: &[usize], payload: &[u8]) -> Vec<u8> {
        let shape_str = if shape.is_empty() {
            "()".to_string()
        } else if shape.len() == 1 {
            format!("({},)", shape[0])
        } else {
            let dims: Vec<String> = shape.iter().map(|d| d.to_string()).collect();
            format!("({})", dims.join(", "))
        };
        let header = format!(
            "{{'descr': '{dtype}', 'fortran_order': False, 'shape': {shape_str}, }}"
        );
        // Pad header so total preamble + header ends on 64-byte boundary.
        let preamble_len = 10;
        let mut header_bytes = header.into_bytes();
        // Leave room for trailing newline.
        let unpadded = preamble_len + header_bytes.len() + 1;
        let pad = (64 - (unpadded % 64)) % 64;
        header_bytes.extend(std::iter::repeat(b' ').take(pad));
        header_bytes.push(b'\n');
        let mut out = Vec::with_capacity(preamble_len + header_bytes.len() + payload.len());
        out.extend_from_slice(&MAGIC);
        out.push(1); // major
        out.push(0); // minor
        out.extend_from_slice(&(header_bytes.len() as u16).to_le_bytes());
        out.extend_from_slice(&header_bytes);
        out.extend_from_slice(payload);
        out
    }

    #[test]
    fn round_trip_f32_1d() {
        let data = [1.0f32, 2.5, -3.7, 4.0];
        let payload: Vec<u8> = data.iter().flat_map(|f| f.to_le_bytes()).collect();
        let npy = synth_npy("<f4", &[4], &payload);
        let read = read_f32(Cursor::new(&npy), "test").expect("read_f32");
        assert_eq!(read.shape, vec![4]);
        assert_eq!(read.data, data);
    }

    #[test]
    fn round_trip_f32_2d() {
        // 2 rows × 3 cols = 6 elements, row-major.
        let data = [1.0f32, 2.0, 3.0, 4.0, 5.0, 6.0];
        let payload: Vec<u8> = data.iter().flat_map(|f| f.to_le_bytes()).collect();
        let npy = synth_npy("<f4", &[2, 3], &payload);
        let read = read_f32(Cursor::new(&npy), "test").expect("read_f32");
        assert_eq!(read.shape, vec![2, 3]);
        assert_eq!(read.data, data);
    }

    #[test]
    fn round_trip_i32() {
        let data = [10i32, -20, 30, 40];
        let payload: Vec<u8> = data.iter().flat_map(|i| i.to_le_bytes()).collect();
        let npy = synth_npy("<i4", &[4], &payload);
        let read = read_i32(Cursor::new(&npy), "test").expect("read_i32");
        assert_eq!(read.data, data);
    }

    #[test]
    fn rejects_wrong_dtype() {
        let npy = synth_npy("<f8", &[1], &[0u8; 8]);
        let err = read_f32(Cursor::new(&npy), "test").unwrap_err();
        assert!(matches!(err, NpyError::WrongDtype { .. }));
    }

    #[test]
    fn rejects_bad_magic() {
        let mut bytes = vec![0u8; 100];
        bytes[..6].copy_from_slice(b"NOTNPY");
        let err = read_f32(Cursor::new(&bytes), "test").unwrap_err();
        assert!(matches!(err, NpyError::BadMagic(_)));
    }

    #[test]
    fn rejects_unsupported_version() {
        // Magic + version=99 + minor=0 + 0-len header.
        let mut bytes = vec![0u8; 100];
        bytes[..6].copy_from_slice(&MAGIC);
        bytes[6] = 99;
        let err = read_f32(Cursor::new(&bytes), "test").unwrap_err();
        assert!(matches!(err, NpyError::UnsupportedVersion { .. }));
    }

    #[test]
    fn parse_header_basic() {
        let h = "{'descr': '<f4', 'fortran_order': False, 'shape': (3, 4), }";
        let (dtype, shape) = parse_header(h, "test").unwrap();
        assert_eq!(dtype, "<f4");
        assert_eq!(shape, vec![3, 4]);
    }

    #[test]
    fn parse_header_1d_with_trailing_comma() {
        let h = "{'descr': '<f4', 'fortran_order': False, 'shape': (52,), }";
        let (_, shape) = parse_header(h, "test").unwrap();
        assert_eq!(shape, vec![52]);
    }

    #[test]
    fn parse_header_zero_dim_treated_as_one() {
        let h = "{'descr': '<f4', 'fortran_order': False, 'shape': (), }";
        let (_, shape) = parse_header(h, "test").unwrap();
        // C# version returns [1] for zero-dim — match that for consumer parity.
        assert_eq!(shape, vec![1]);
    }
}
