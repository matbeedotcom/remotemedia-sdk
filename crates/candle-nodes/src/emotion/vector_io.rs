//! Vector I/O — load and save emotion direction vectors
//!
//! Vectors are stored as raw f32 bytes with a small JSON metadata
//! header. This format is compatible with PyTorch `.pt` files
//! (via `torch.load(..., map_location="cpu")` → `.numpy().tobytes()`)
//! and can be read by any system that understands flat binary arrays.
//!
//! # File format
//!
//! ```text
//! ┌─────────────────────┬──────────────────────────┐
//! │  4 bytes (u32 LE)   │  JSON metadata length    │
//! ├─────────────────────┼──────────────────────────┤
//! │  N bytes            │  JSON metadata (EmotionVectorMetadata) │
//! ├─────────────────────┼──────────────────────────┤
//! │  M × 4 bytes        │  f32 vector data (row-major) │
//! └─────────────────────┴──────────────────────────┘
//! ```

use super::config::EmotionVectorMetadata;
use crate::error::{CandleNodeError, Result};
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

/// Load an emotion vector from a `.bin` file.
///
/// Returns the f32 vector and its metadata.
pub fn load_emotion_vector(path: impl AsRef<Path>) -> Result<(Vec<f32>, EmotionVectorMetadata)> {
    let path = path.as_ref();
    if !path.exists() {
        return Err(CandleNodeError::configuration(
            "emotion-vector",
            format!("Vector file not found: {}", path.display()),
        ));
    }

    let mut data = fs::read(path)
        .map_err(|e| CandleNodeError::configuration("emotion-vector", e.to_string()))?;

    if data.len() < 4 {
        return Err(CandleNodeError::configuration(
            "emotion-vector",
            "File too small for metadata header",
        ));
    }

    // Read metadata length
    let meta_len = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
    data.drain(..4);

    if data.len() < meta_len {
        return Err(CandleNodeError::configuration(
            "emotion-vector",
            format!(
                "File too small: claimed metadata {} bytes, only {} remaining",
                meta_len,
                data.len()
            ),
        ));
    }

    // Parse JSON metadata
    let meta_json: &[u8] = &data[..meta_len];
    let metadata: EmotionVectorMetadata =
        serde_json::from_slice(meta_json).map_err(|e| CandleNodeError::configuration(
            "emotion-vector",
            format!("Invalid metadata JSON: {}", e),
        ))?;
    data.drain(..meta_len);

    // Parse f32 vector
    if data.len() % 4 != 0 {
        return Err(CandleNodeError::configuration(
            "emotion-vector",
            format!("Vector data length {} is not a multiple of 4", data.len()),
        ));
    }

    let n_elements = data.len() / 4;
    let expected = metadata.hidden_size;
    if n_elements != expected {
        return Err(CandleNodeError::configuration(
            "emotion-vector",
            format!(
                "Vector has {} elements but metadata says hidden_size={}",
                n_elements, expected
            ),
        ));
    }

    let mut vector = Vec::with_capacity(n_elements);
    for chunk in data.chunks_exact(4) {
        let val = f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        vector.push(val);
    }

    Ok((vector, metadata))
}

/// Save an emotion vector to a `.bin` file.
pub fn save_emotion_vector(
    path: impl AsRef<Path>,
    vector: &[f32],
    metadata: &EmotionVectorMetadata,
) -> Result<()> {
    let path = path.as_ref();

    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| CandleNodeError::configuration(
            "emotion-vector",
            format!("Failed to create directory {}: {}", parent.display(), e),
        ))?;
    }

    // Serialize metadata
    let meta_json = serde_json::to_vec(metadata).map_err(|e| CandleNodeError::configuration(
        "emotion-vector",
        format!("Failed to serialize metadata: {}", e),
    ))?;

    // Validate shape
    if vector.len() != metadata.hidden_size {
        return Err(CandleNodeError::configuration(
            "emotion-vector",
            format!(
                "Vector length {} != metadata hidden_size {}",
                vector.len(),
                metadata.hidden_size
            ),
        ));
    }

    // Write file
    let mut file = fs::File::create(path).map_err(|e| CandleNodeError::configuration(
        "emotion-vector",
        format!("Failed to create file {}: {}", path.display(), e),
    ))?;

    // Metadata length (u32 LE)
    file.write_all(&meta_len_to_le(meta_json.len()))
        .map_err(io_error_to_candle("emotion-vector"))?;

    // Metadata JSON
    file.write_all(&meta_json).map_err(io_error_to_candle("emotion-vector"))?;

    // f32 vector data (LE)
    for &val in vector {
        file.write_all(&val.to_le_bytes())
            .map_err(io_error_to_candle("emotion-vector"))?;
    }

    Ok(())
}

/// Convert a metadata length to little-endian u32 bytes.
fn meta_len_to_le(len: usize) -> [u8; 4] {
    (len as u32).to_le_bytes()
}

/// Convert an I/O error to a CandleNodeError.
fn io_error_to_candle(node_id: &'static str) -> impl FnOnce(io::Error) -> CandleNodeError {
    move |e: io::Error| CandleNodeError::configuration(node_id, e.to_string())
}

/// Compute the mean of a list of f32 vectors (all same length).
pub fn mean_vectors(vectors: &[Vec<f32>]) -> Result<Vec<f32>> {
    if vectors.is_empty() {
        return Err(CandleNodeError::configuration(
            "emotion-vector",
            "Cannot compute mean of empty vector list",
        ));
    }

    let n = vectors[0].len();
    for (i, v) in vectors.iter().enumerate() {
        if v.len() != n {
            return Err(CandleNodeError::configuration(
                "emotion-vector",
                format!(
                    "Vector {} has length {}, expected {}",
                    i,
                    v.len(),
                    n
                ),
            ));
        }
    }

    let count = vectors.len() as f32;
    let mut mean = vec![0.0f32; n];

    for v in vectors {
        for (m, &val) in mean.iter_mut().zip(v) {
            *m += val;
        }
    }

    for m in &mut mean {
        *m /= count;
    }

    Ok(mean)
}

/// Subtract two vectors element-wise: a - b.
pub fn subtract_vectors(a: &[f32], b: &[f32]) -> Result<Vec<f32>> {
    if a.len() != b.len() {
        return Err(CandleNodeError::configuration(
            "emotion-vector",
            format!(
                "Cannot subtract vectors of different lengths: {} vs {}",
                a.len(),
                b.len()
            ),
        ));
    }

    Ok(a.iter().zip(b).map(|(&x, &y)| x - y).collect())
}

/// L2-normalize a vector in-place and return the original norm.
pub fn l2_normalize(vector: &mut Vec<f32>) -> f32 {
    let norm: f32 = vector.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for v in vector.iter_mut() {
            *v /= norm;
        }
    }
    norm
}

/// Compute cosine similarity between two vectors.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> Result<f32> {
    if a.len() != b.len() {
        return Err(CandleNodeError::configuration(
            "emotion-vector",
            "Vectors must have the same length for cosine similarity",
        ));
    }

    let dot: f32 = a.iter().zip(b).map(|(&x, &y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if norm_a == 0.0 || norm_b == 0.0 {
        return Ok(0.0);
    }

    Ok(dot / (norm_a * norm_b))
}

/// Scale a vector by a scalar: vector × coef × layer_norm.
pub fn scale_for_steering(vector: &[f32], coefficient: f32, layer_norm: f32) -> Vec<f32> {
    let scale = coefficient * layer_norm;
    vector.iter().map(|&x| x * scale).collect()
}

/// Compute the steering delta: Σ(coef_i × layer_norm × vec_i).
///
/// This is the single vector that gets added to the residual stream.
pub fn compute_steering_delta(
    vectors: &[Vec<f32>],
    coefficients: &[f32],
    layer_norm: f32,
    hidden_size: usize,
) -> Result<Vec<f32>> {
    if vectors.len() != coefficients.len() {
        return Err(CandleNodeError::configuration(
            "emotion-steering",
            format!(
                "Vector count ({}) != coefficient count ({})",
                vectors.len(),
                coefficients.len()
            ),
        ));
    }

    let mut delta = vec![0.0f32; hidden_size];

    for (vec, &coef) in vectors.iter().zip(coefficients) {
        if vec.len() != hidden_size {
            return Err(CandleNodeError::configuration(
                "emotion-steering",
                format!(
                    "Vector length {} != hidden_size {}",
                    vec.len(),
                    hidden_size
                ),
            ));
        }
        let scale = coef * layer_norm;
        for (d, &v) in delta.iter_mut().zip(vec) {
            *d += v * scale;
        }
    }

    Ok(delta)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_mean_vectors() {
        let vectors = vec![
            vec![1.0, 2.0, 3.0],
            vec![3.0, 4.0, 5.0],
            vec![5.0, 6.0, 7.0],
        ];
        let mean = mean_vectors(&vectors).unwrap();
        assert_eq!(mean, vec![3.0, 4.0, 5.0]);
    }

    #[test]
    fn test_subtract_vectors() {
        let a = vec![5.0, 6.0, 7.0];
        let b = vec![3.0, 4.0, 5.0];
        let diff = subtract_vectors(&a, &b).unwrap();
        assert_eq!(diff, vec![2.0, 2.0, 2.0]);
    }

    #[test]
    fn test_l2_normalize() {
        let mut v = vec![3.0, 4.0];
        let norm = l2_normalize(&mut v);
        assert!((norm - 5.0).abs() < 1e-6);
        assert!((v[0] - 0.6).abs() < 1e-6);
        assert!((v[1] - 0.8).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_identical() {
        let a = vec![1.0, 2.0, 3.0];
        let sim = cosine_similarity(&a, &a).unwrap();
        assert!((sim - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_opposite() {
        let a = vec![1.0, 0.0];
        let b = vec![-1.0, 0.0];
        let sim = cosine_similarity(&a, &b).unwrap();
        assert!((sim - (-1.0)).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_orthogonal() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        let sim = cosine_similarity(&a, &b).unwrap();
        assert!((sim - 0.0).abs() < 1e-6);
    }

    #[test]
    fn test_scale_for_steering() {
        let v = vec![1.0, 2.0, 3.0];
        let scaled = scale_for_steering(&v, 0.5, 10.0);
        assert_eq!(scaled, vec![5.0, 10.0, 15.0]);
    }

    #[test]
    fn test_compute_steering_delta() {
        let vectors = vec![vec![1.0, 0.0], vec![0.0, 1.0]];
        let coefs = vec![0.5, 0.3];
        let delta = compute_steering_delta(&vectors, &coefs, 10.0, 2).unwrap();
        // 0.5 * 10 * [1, 0] + 0.3 * 10 * [0, 1] = [5, 3]
        assert!((delta[0] - 5.0).abs() < 1e-6);
        assert!((delta[1] - 3.0).abs() < 1e-6);
    }

    #[test]
    fn test_save_and_load_roundtrip() {
        let metadata = EmotionVectorMetadata {
            model: "test-model".to_string(),
            layer: 21,
            hidden_size: 4,
            emotion: "happy".to_string(),
            pooling: super::super::config::PoolingMode::LastToken,
            n_positive: 10,
            n_neutral: 10,
            raw_norm: 5.0,
            dataset_hash: "abc123".to_string(),
            system_prompt: "".to_string(),
            normalized: true,
        };
        let vector = vec![0.5, -0.3, 0.1, 0.8];

        let path = "/tmp/test_emotion_vector.bin";
        save_emotion_vector(path, &vector, &metadata).unwrap();

        let (loaded_vec, loaded_meta) = load_emotion_vector(path).unwrap();
        assert_eq!(loaded_vec, vector);
        assert_eq!(loaded_meta.emotion, "happy");
        assert_eq!(loaded_meta.layer, 21);
        assert_eq!(loaded_meta.hidden_size, 4);

        // Cleanup
        let _ = fs::remove_file(path);
    }

    #[test]
    fn test_load_nonexistent_file() {
        let result = load_emotion_vector("/tmp/does_not_exist_12345.bin");
        assert!(result.is_err());
    }
}
