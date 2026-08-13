use crate::error::{Error, Result};

/// Serialize an embedding to its little-endian f32 BLOB.
pub fn embedding_to_blob(v: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(v.len() * 4);
    for f in v {
        bytes.extend_from_slice(&f.to_le_bytes());
    }
    bytes
}

/// Deserialize a `chunks.embedding` BLOB back to f32s. Dimension is not fixed
/// (the active model may be 384/768/1024-dim); require only a non-empty,
/// 4-byte-aligned blob. Cross-model comparison is prevented upstream.
pub fn blob_to_embedding(blob: &[u8]) -> Result<Vec<f32>> {
    if blob.is_empty() || blob.len() % 4 != 0 {
        return Err(Error::InvalidInput(format!(
            "embedding blob is {} bytes, not a whole number of f32s",
            blob.len()
        )));
    }
    Ok(blob
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blob_roundtrip() {
        let v = vec![1.0f32, -0.5, 0.0, 3.25];
        let back = blob_to_embedding(&embedding_to_blob(&v)).unwrap();
        assert_eq!(back, v);
    }

    #[test]
    fn blob_rejects_misaligned() {
        assert!(blob_to_embedding(&[1, 2, 3]).is_err());
        assert!(blob_to_embedding(&[]).is_err());
    }
}
