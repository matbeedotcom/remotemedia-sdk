//! Minimal `.npz` archive reader.
//!
//! `.npz` is just a ZIP file containing one `.npy` per saved array.
//! NumPy's `np.savez(path, name=array)` produces entries like
//! `name.npy`. The persona-engine bundle's NPZs follow that
//! convention (`neutral.npy`, `frontalMask.npy`, `eyeBlinkLeft.npy`,
//! …), so we just open the ZIP and dispatch each entry through
//! [`super::npy`].
//!
//! No streaming abstraction here on purpose — the archives are
//! ≤ 15 MB compressed and we read each entry once at startup. Flat
//! API beats lifetime gymnastics.

use super::npy::{self, NpyError, NpyF32, NpyI32};
use std::fs::File;
use std::io::{Read, Seek};
use std::path::Path;

/// Errors surfaced by the NPZ archive layer.
#[derive(Debug, thiserror::Error)]
pub enum NpzError {
    #[error("io error opening {1}: {0}")]
    Io(#[source] std::io::Error, String),

    #[error("zip error in {1}: {0}")]
    Zip(#[source] zip::result::ZipError, String),

    #[error("npy error in {1}: {0}")]
    Npy(#[source] NpyError, String),

    #[error("entry '{0}' not found in NPZ {1}")]
    EntryMissing(String, String),
}

/// Open + dispatch over an `.npz` archive.
pub struct NpzArchive<R: Read + Seek> {
    archive: zip::ZipArchive<R>,
    /// Source path / id, surfaced in errors for diagnostics.
    source: String,
}

impl NpzArchive<File> {
    /// Open `.npz` from a filesystem path.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, NpzError> {
        let path = path.as_ref();
        let source = path.display().to_string();
        let file = File::open(path).map_err(|e| NpzError::Io(e, source.clone()))?;
        let archive = zip::ZipArchive::new(file).map_err(|e| NpzError::Zip(e, source.clone()))?;
        Ok(Self { archive, source })
    }
}

impl<R: Read + Seek> NpzArchive<R> {
    /// Build from any `Read + Seek` source (e.g. an in-memory cursor
    /// for tests).
    pub fn from_reader(reader: R, source_name: impl Into<String>) -> Result<Self, NpzError> {
        let source = source_name.into();
        let archive = zip::ZipArchive::new(reader).map_err(|e| NpzError::Zip(e, source.clone()))?;
        Ok(Self { archive, source })
    }

    /// True iff the archive has an entry by the exact name.
    pub fn has_entry(&mut self, name: &str) -> bool {
        self.archive.by_name(name).is_ok()
    }

    /// Read an `.npy` entry as float32. The entry name should include
    /// the `.npy` suffix (NumPy's `savez` always writes that suffix).
    pub fn read_f32(&mut self, name: &str) -> Result<NpyF32, NpzError> {
        let mut entry = self.archive.by_name(name).map_err(|e| match e {
            zip::result::ZipError::FileNotFound => {
                NpzError::EntryMissing(name.to_string(), self.source.clone())
            }
            other => NpzError::Zip(other, self.source.clone()),
        })?;
        npy::read_f32(&mut entry, name).map_err(|e| NpzError::Npy(e, name.to_string()))
    }

    /// Read an `.npy` entry as int32.
    pub fn read_i32(&mut self, name: &str) -> Result<NpyI32, NpzError> {
        let mut entry = self.archive.by_name(name).map_err(|e| match e {
            zip::result::ZipError::FileNotFound => {
                NpzError::EntryMissing(name.to_string(), self.source.clone())
            }
            other => NpzError::Zip(other, self.source.clone()),
        })?;
        npy::read_i32(&mut entry, name).map_err(|e| NpzError::Npy(e, name.to_string()))
    }

    /// List every entry name in the archive (for diagnostics +
    /// fixture introspection).
    pub fn entry_names(&self) -> Vec<String> {
        self.archive.file_names().map(|s| s.to_string()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Write};
    use zip::write::SimpleFileOptions;

    /// Build a minimal valid v1 .npy byte stream — copied from
    /// npy::tests::synth_npy.
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
        let preamble_len = 10;
        let mut header_bytes = header.into_bytes();
        let unpadded = preamble_len + header_bytes.len() + 1;
        let pad = (64 - (unpadded % 64)) % 64;
        header_bytes.extend(std::iter::repeat(b' ').take(pad));
        header_bytes.push(b'\n');
        let mut out = Vec::new();
        out.extend_from_slice(&[0x93, b'N', b'U', b'M', b'P', b'Y']);
        out.push(1);
        out.push(0);
        out.extend_from_slice(&(header_bytes.len() as u16).to_le_bytes());
        out.extend_from_slice(&header_bytes);
        out.extend_from_slice(payload);
        out
    }

    /// Build an in-memory NPZ archive for testing.
    fn build_npz(entries: Vec<(&str, Vec<u8>)>) -> Vec<u8> {
        let mut buf = Cursor::new(Vec::new());
        {
            let mut zw = zip::ZipWriter::new(&mut buf);
            for (name, body) in &entries {
                zw.start_file(*name, SimpleFileOptions::default())
                    .expect("start_file");
                zw.write_all(body).expect("write_all");
            }
            zw.finish().expect("zip finish");
        }
        buf.into_inner()
    }

    #[test]
    fn open_in_memory_archive_and_read_entries() {
        let payload_a: Vec<u8> = [1.0f32, 2.0, 3.0]
            .iter()
            .flat_map(|f| f.to_le_bytes())
            .collect();
        let entry_a = synth_npy("<f4", &[3], &payload_a);

        let payload_b: Vec<u8> = [10i32, 20].iter().flat_map(|i| i.to_le_bytes()).collect();
        let entry_b = synth_npy("<i4", &[2], &payload_b);

        let zip_bytes = build_npz(vec![("alpha.npy", entry_a), ("beta.npy", entry_b)]);

        let mut archive =
            NpzArchive::from_reader(Cursor::new(zip_bytes), "test.npz").expect("open");
        let names = archive.entry_names();
        assert!(names.contains(&"alpha.npy".to_string()));
        assert!(names.contains(&"beta.npy".to_string()));

        let alpha = archive.read_f32("alpha.npy").expect("alpha");
        assert_eq!(alpha.data, vec![1.0, 2.0, 3.0]);
        assert_eq!(alpha.shape, vec![3]);

        let beta = archive.read_i32("beta.npy").expect("beta");
        assert_eq!(beta.data, vec![10, 20]);
    }

    #[test]
    fn missing_entry_returns_actionable_error() {
        let zip_bytes = build_npz(vec![]);
        let mut archive =
            NpzArchive::from_reader(Cursor::new(zip_bytes), "test.npz").expect("open");
        let err = archive.read_f32("nope.npy").unwrap_err();
        match err {
            NpzError::EntryMissing(name, source) => {
                assert_eq!(name, "nope.npy");
                assert_eq!(source, "test.npz");
            }
            other => panic!("expected EntryMissing, got {other:?}"),
        }
    }

    #[test]
    fn has_entry_works() {
        let payload: Vec<u8> = [1.0f32].iter().flat_map(|f| f.to_le_bytes()).collect();
        let entry = synth_npy("<f4", &[1], &payload);
        let zip_bytes = build_npz(vec![("x.npy", entry)]);
        let mut archive =
            NpzArchive::from_reader(Cursor::new(zip_bytes), "test.npz").expect("open");
        assert!(archive.has_entry("x.npy"));
        assert!(!archive.has_entry("y.npy"));
    }
}
