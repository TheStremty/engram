use anyhow::{Context, Result};
use fastembed::{EmbeddingModel, TextEmbedding, TextInitOptions};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

const VECTORS_FILE: &str = "vectors.json";

#[derive(Serialize, Deserialize, Default, Clone)]
pub struct VectorStore {
    pub vectors: HashMap<String, Vec<f32>>,
}

impl VectorStore {
    pub fn load(vault_path: &Path) -> Result<Self> {
        let path = get_vectors_path(vault_path);
        if !path.exists() {
            return Ok(Self::default());
        }
        let data = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read vector store: {}", path.display()))?;
        let store = serde_json::from_str(&data).unwrap_or_default();
        Ok(store)
    }

    pub fn save(&self, vault_path: &Path) -> Result<()> {
        let path = get_vectors_path(vault_path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string(self)?;
        fs::write(&path, json)
            .with_context(|| format!("Failed to save vector store: {}", path.display()))?;
        Ok(())
    }

    pub fn insert(&mut self, filename: String, vector: Vec<f32>) {
        self.vectors.insert(filename, vector);
    }

    pub fn remove(&mut self, filename: &str) {
        self.vectors.remove(filename);
    }
}

pub fn get_vectors_path(vault_path: &Path) -> PathBuf {
    vault_path.join(".eng_index").join(VECTORS_FILE)
}

pub fn get_embedder() -> Result<TextEmbedding> {
    let options = TextInitOptions::new(EmbeddingModel::AllMiniLML6V2);
    TextEmbedding::try_new(options).context("Failed to initialize FastEmbed ONNX model")
}

pub fn embed_texts(texts: Vec<String>) -> Result<Vec<Vec<f32>>> {
    if texts.is_empty() {
        return Ok(Vec::new());
    }
    let mut model = get_embedder()?;
    let embeddings = model.embed(texts, None)?;
    Ok(embeddings)
}

pub fn embed_text(text: &str) -> Result<Vec<f32>> {
    let embeddings = embed_texts(vec![text.to_string()])?;
    embeddings
        .into_iter()
        .next()
        .context("No embedding generated")
}

pub fn embed_single_note(vault_path: &Path, note: &crate::vault::Note) -> Result<()> {
    let mut store = VectorStore::load(vault_path)?;
    let text_to_embed = format!(
        "{}\n{}\n{}",
        note.filename,
        note.frontmatter.tags.join(" "),
        note.content
    );
    let vector = embed_text(&text_to_embed)?;
    store.insert(note.filename.clone(), vector);
    store.save(vault_path)?;
    Ok(())
}

pub fn embed_vault(vault_path: &Path, force: bool) -> Result<usize> {
    let mut store = VectorStore::load(vault_path)?;
    let files = crate::vault::list_markdown_files(vault_path)?;

    let mut pending_files = Vec::new();
    let mut pending_texts = Vec::new();

    for file in files {
        if let Ok(note) = crate::vault::read_note(&file) {
            if force || !store.vectors.contains_key(&note.filename) {
                let text = format!(
                    "{}\n{}\n{}",
                    note.filename,
                    note.frontmatter.tags.join(" "),
                    note.content
                );
                pending_files.push(note.filename);
                pending_texts.push(text);
            }
        }
    }

    if pending_files.is_empty() {
        return Ok(0);
    }

    let count = pending_files.len();
    let vectors = embed_texts(pending_texts)?;

    for (filename, vec) in pending_files.into_iter().zip(vectors.into_iter()) {
        store.insert(filename, vec);
    }

    store.save(vault_path)?;
    Ok(count)
}

pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        0.0
    } else {
        dot / (norm_a * norm_b)
    }
}

pub struct SemanticScore {
    pub file: String,
    pub score: f32,
}

pub fn search_semantic(
    vault_path: &Path,
    query_str: &str,
    limit: usize,
) -> Result<Vec<SemanticScore>> {
    let store = VectorStore::load(vault_path)?;
    if store.vectors.is_empty() {
        return Ok(Vec::new());
    }

    let query_vector = embed_text(query_str)?;

    let mut scores = Vec::new();
    for (file, doc_vec) in &store.vectors {
        let sim = cosine_similarity(&query_vector, doc_vec);
        scores.push(SemanticScore {
            file: file.clone(),
            score: sim,
        });
    }

    scores.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    scores.truncate(limit);
    Ok(scores)
}

/// Reciprocal Rank Fusion (RRF) combining BM25 results with Semantic Cosine Similarity
pub fn hybrid_fuse(
    bm25_results: &[crate::index::SearchResult],
    semantic_results: &[SemanticScore],
    k: f32, // RRF constant, usually 60.0
) -> Vec<(String, f32)> {
    let mut score_map: HashMap<String, f32> = HashMap::new();

    for (rank, hit) in bm25_results.iter().enumerate() {
        let rrf = 1.0 / (k + (rank + 1) as f32);
        *score_map.entry(hit.file.clone()).or_insert(0.0) += rrf;
    }

    for (rank, hit) in semantic_results.iter().enumerate() {
        let rrf = 1.0 / (k + (rank + 1) as f32);
        *score_map.entry(hit.file.clone()).or_insert(0.0) += rrf;
    }

    let mut fused: Vec<(String, f32)> = score_map.into_iter().collect();
    fused.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    fused
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cosine_similarity() {
        let v1 = vec![1.0, 0.0, 0.0];
        let v2 = vec![1.0, 0.0, 0.0];
        let v3 = vec![0.0, 1.0, 0.0];

        assert!((cosine_similarity(&v1, &v2) - 1.0).abs() < 1e-5);
        assert!((cosine_similarity(&v1, &v3) - 0.0).abs() < 1e-5);
    }
}
