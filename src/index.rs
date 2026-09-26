use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use tantivy::collector::TopDocs;
use tantivy::query::QueryParser;
use tantivy::schema::*;
use tantivy::{doc, Index, IndexWriter, ReloadPolicy, TantivyDocument, Term};

use crate::vault::{list_markdown_files, read_note, Note};

pub const INDEX_DIR_NAME: &str = ".eng_index";

#[derive(Clone, Copy)]
pub struct SchemaFields {
    pub path: Field,
    pub title: Field,
    pub content: Field,
    pub tags: Field,
}

pub struct SearchResult {
    pub file: String,
    pub score: f32,
    pub content: String,
}

pub struct IndexStatus {
    pub doc_count: u64,
    pub size_bytes: u64,
    pub ready: bool,
}

fn build_schema() -> (Schema, SchemaFields) {
    let mut schema_builder = Schema::builder();
    let path = schema_builder.add_text_field("path", STRING | STORED);
    let title = schema_builder.add_text_field("title", TEXT | STORED);
    let content = schema_builder.add_text_field("content", TEXT | STORED);
    let tags = schema_builder.add_text_field("tags", TEXT | STORED);
    let schema = schema_builder.build();
    (
        schema,
        SchemaFields {
            path,
            title,
            content,
            tags,
        },
    )
}

fn get_fields(schema: &Schema) -> Result<SchemaFields> {
    let path = schema.get_field("path")?;
    let title = schema.get_field("title")?;
    let content = schema.get_field("content")?;
    let tags = schema.get_field("tags")?;
    Ok(SchemaFields {
        path,
        title,
        content,
        tags,
    })
}

pub fn open_or_create_index(index_dir: &Path) -> Result<(Index, SchemaFields)> {
    fs::create_dir_all(index_dir).context("Failed to create index directory")?;

    let (schema, fields) = build_schema();
    let index = if index_dir.join("meta.json").exists() {
        let opened = Index::open_in_dir(index_dir)?;
        let schema = opened.schema();
        let loaded_fields = get_fields(&schema)?;
        return Ok((opened, loaded_fields));
    } else {
        Index::create_in_dir(index_dir, schema)?
    };

    Ok((index, fields))
}

pub fn get_index_path(vault_path: &Path) -> PathBuf {
    vault_path.join(INDEX_DIR_NAME)
}

/// Acquires an IndexWriter with exponential backoff retry to handle concurrent subagent processes.
pub fn get_writer_with_retry(index: &Index, memory: usize) -> Result<IndexWriter> {
    use std::thread::sleep;
    use std::time::{Duration, Instant};

    let timeout = Duration::from_secs(6);
    let start = Instant::now();
    let mut backoff = Duration::from_millis(25);

    loop {
        match index.writer(memory) {
            Ok(writer) => return Ok(writer),
            Err(tantivy::TantivyError::LockFailure(e, _)) => {
                if start.elapsed() >= timeout {
                    anyhow::bail!("Failed to acquire index writer lock after 6s: {}", e);
                }
                sleep(backoff);
                backoff = (backoff * 2).min(Duration::from_millis(250));
            }
            Err(e) => return Err(e.into()),
        }
    }
}

pub fn index_single_note(index: &Index, fields: &SchemaFields, note: &Note) -> Result<()> {
    let mut writer: IndexWriter = get_writer_with_retry(index, 15_000_000)?;
    let term = Term::from_field_text(fields.path, &note.filename);
    writer.delete_term(term);

    let title = note.filename.strip_suffix(".md").unwrap_or(&note.filename);
    let tags_str = note.frontmatter.tags.join(" ");

    let doc = doc!(
        fields.path => note.filename.as_str(),
        fields.title => title,
        fields.content => note.content.as_str(),
        fields.tags => tags_str.as_str(),
    );

    writer.add_document(doc)?;
    writer.commit()?;
    Ok(())
}

pub fn delete_single_note(index: &Index, fields: &SchemaFields, filename: &str) -> Result<()> {
    let mut writer: IndexWriter = get_writer_with_retry(index, 15_000_000)?;
    let term = Term::from_field_text(fields.path, filename);
    writer.delete_term(term);
    writer.commit()?;
    Ok(())
}

pub fn sync_all_notes(vault_path: &Path, force: bool) -> Result<usize> {
    let index_dir = get_index_path(vault_path);

    if force && index_dir.exists() {
        let _ = fs::remove_dir_all(&index_dir);
    }

    let (index, fields) = open_or_create_index(&index_dir)?;
    let mut writer: IndexWriter = get_writer_with_retry(&index, 30_000_000)?;
    writer.delete_all_documents()?;

    let files = list_markdown_files(vault_path)?;
    let mut count = 0;

    for file in &files {
        if let Ok(note) = read_note(file) {
            let title = note.filename.strip_suffix(".md").unwrap_or(&note.filename);
            let tags_str = note.frontmatter.tags.join(" ");

            let doc = doc!(
                fields.path => note.filename.as_str(),
                fields.title => title,
                fields.content => note.content.as_str(),
                fields.tags => tags_str.as_str(),
            );
            writer.add_document(doc)?;
            count += 1;
        }
    }

    writer.commit()?;
    Ok(count)
}

pub fn search_vault(vault_path: &Path, query_str: &str, limit: usize) -> Result<Vec<SearchResult>> {
    let index_dir = get_index_path(vault_path);
    let (index, fields) = open_or_create_index(&index_dir)?;

    let reader = index
        .reader_builder()
        .reload_policy(ReloadPolicy::OnCommitWithDelay)
        .try_into()?;
    let searcher = reader.searcher();

    let query_parser =
        QueryParser::for_index(&index, vec![fields.title, fields.content, fields.tags]);

    // Parse query, fall back to sanitized if user syntax causes error
    let query = match query_parser.parse_query(query_str) {
        Ok(q) => q,
        Err(_) => {
            // Strip special tantivy query syntax characters
            let sanitized: String = query_str
                .chars()
                .filter(|c| c.is_alphanumeric() || c.is_whitespace())
                .collect();
            query_parser
                .parse_query(&sanitized)
                .context("Failed to parse query even after sanitizing")?
        }
    };

    let top_docs = searcher.search(&query, &TopDocs::with_limit(limit))?;
    let mut results = Vec::new();

    for (score, doc_address) in top_docs {
        let doc: TantivyDocument = searcher.doc(doc_address)?;
        let file = doc
            .get_first(fields.path)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let content = doc
            .get_first(fields.content)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        results.push(SearchResult {
            file,
            score,
            content,
        });
    }

    Ok(results)
}

pub fn get_index_status(vault_path: &Path) -> Result<IndexStatus> {
    let index_dir = get_index_path(vault_path);
    if !index_dir.exists() {
        return Ok(IndexStatus {
            doc_count: 0,
            size_bytes: 0,
            ready: false,
        });
    }

    let mut size_bytes = 0;
    if let Ok(entries) = fs::read_dir(&index_dir) {
        for entry in entries.flatten() {
            if let Ok(meta) = entry.metadata() {
                size_bytes += meta.len();
            }
        }
    }

    if let Ok((index, _)) = open_or_create_index(&index_dir) {
        if let Ok(reader) = index.reader() {
            let searcher = reader.searcher();
            let doc_count = searcher.num_docs();
            return Ok(IndexStatus {
                doc_count,
                size_bytes,
                ready: true,
            });
        }
    }

    Ok(IndexStatus {
        doc_count: 0,
        size_bytes,
        ready: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_index_and_search() {
        let dir = tempdir().unwrap();
        let vault_path = dir.path();
        let index_dir = get_index_path(vault_path);
        let (index, fields) = open_or_create_index(&index_dir).unwrap();

        let note = Note {
            filename: "rust-tips.md".to_string(),
            path: vault_path.join("rust-tips.md"),
            frontmatter: crate::vault::NoteFrontmatter {
                tags: vec!["rust".to_string(), "performance".to_string()],
            },
            content: "Zero-cost abstractions in Rust make it very fast and memory safe."
                .to_string(),
            raw: String::new(),
        };

        index_single_note(&index, &fields, &note).unwrap();

        let results = search_vault(vault_path, "zero-cost safe", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].file, "rust-tips.md");
        assert!(results[0].score > 0.0);
    }
}
