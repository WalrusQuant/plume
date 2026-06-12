//! SPIKE — local on-device text embeddings via fastembed (ONNX Runtime).
//! Goal: prove the model loads, embeds text, and that cosine similarity
//! separates related from unrelated text — all in-process, no API key.
//! If this builds + passes, the rest of semantic search is routine.

#[cfg(test)]
mod spike {
    use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};

    fn cosine(a: &[f32], b: &[f32]) -> f32 {
        let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
        let na: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let nb: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
        dot / (na * nb)
    }

    // Spike: hits the network (~90MB model download) and takes ~11s, so it is
    // excluded from the normal fast suite. Run explicitly with:
    //   cargo test --lib embeddings -- --ignored --nocapture
    #[test]
    #[ignore = "downloads the embedding model; run explicitly"]
    fn embeds_text_and_cosine_separates_related_from_unrelated() {
        let model = TextEmbedding::try_new(
            InitOptions::new(EmbeddingModel::BGESmallENV15).with_show_download_progress(true),
        )
        .expect("model should initialize");

        let docs = vec![
            "The cat sat on the warm windowsill in the afternoon sun.",
            "A kitten napped by the sunny window all afternoon.",
            "Quarterly revenue projections exceeded the finance team's forecast.",
        ];
        let embeddings = model.embed(docs, None).expect("embedding should succeed");

        assert_eq!(embeddings.len(), 3);
        assert_eq!(embeddings[0].len(), 384, "BGE-small is 384-dimensional");

        let related = cosine(&embeddings[0], &embeddings[1]); // two cat/window sentences
        let unrelated = cosine(&embeddings[0], &embeddings[2]); // cat vs. finance

        println!("related cosine = {related:.4}, unrelated cosine = {unrelated:.4}");
        assert!(
            related > unrelated,
            "semantically related text must score higher ({related:.4} !> {unrelated:.4})"
        );
    }
}
