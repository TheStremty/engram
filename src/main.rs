mod index;
mod semantic;
mod tokens;
mod vault;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use serde::Serialize;
use std::fs;
use std::io::{IsTerminal, Read};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

#[derive(Parser)]
#[command(
    name = "eng",
    version,
    about = "Ultra-lightweight deterministic memory engine for terminal agents",
    long_about = None,
    arg_required_else_help = true
)]
struct Cli {
    #[arg(
        long,
        global = true,
        env = "ENG_VAULT_PATH",
        help = "Path to the notes vault (default: $ENG_VAULT_PATH, or ~/eng_vault)"
    )]
    vault: Option<PathBuf>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize vault directory and Tantivy search index
    Init {
        #[arg(help = "Optional custom vault directory path")]
        path: Option<PathBuf>,
    },

    /// Search and retrieve note snippet matching query via BM25
    Get {
        #[arg(help = "Search query string")]
        query: String,

        #[arg(
            short,
            long,
            default_value_t = 200,
            help = "Hard token limit on stdout output"
        )]
        tokens: usize,

        #[arg(
            long,
            default_value_t = 1,
            help = "Number of top matching notes to combine within token budget (RAG packing)"
        )]
        top: usize,

        #[arg(
            long,
            help = "Return only a numbered list of matching files and BM25 scores"
        )]
        list: bool,

        #[arg(
            long,
            help = "Extract a specific section/heading from the best matching note"
        )]
        section: Option<String>,

        #[arg(
            long,
            help = "Use hybrid search (BM25 + local vector embeddings via RRF)"
        )]
        hybrid: bool,

        #[arg(long, help = "Use dense vector semantic search only")]
        semantic: bool,

        #[arg(
            long,
            default_value = "text",
            value_enum,
            help = "Output format: text (clean snippet) or json"
        )]
        format: FormatChoice,
    },

    /// Generate or update local vector embeddings for semantic search
    Embed {
        #[arg(long, help = "Force re-generating embeddings for all notes")]
        force: bool,
    },

    /// Read a specific note directly by name without BM25 search
    #[command(alias = "cat", alias = "view")]
    Read {
        #[arg(help = "Note name or filename (with or without .md)")]
        note_name: String,

        #[arg(short, long, help = "Optional token limit for output")]
        tokens: Option<usize>,

        #[arg(long, help = "Extract a specific section/heading from the note")]
        section: Option<String>,

        #[arg(
            long,
            default_value = "text",
            value_enum,
            help = "Output format: text (clean snippet) or json"
        )]
        format: FormatChoice,
    },

    /// Save or append note content (supports STDIN with '-') and index it
    Push {
        #[arg(help = "Note content to store (pass '-' to read from STDIN)")]
        content: String,

        #[arg(short, long, help = "Optional title / filename for the note")]
        title: Option<String>,

        #[arg(long, help = "Comma-separated tags (saved in YAML frontmatter)")]
        tags: Option<String>,

        #[arg(
            short,
            long,
            help = "Append content if note already exists instead of overwriting"
        )]
        append: bool,
    },

    /// Edit a note interactively or programmatically (section, find-replace, body, preserving tags)
    Edit {
        #[arg(help = "Note name or filename (with or without .md)")]
        note_name: String,

        #[arg(short, long, help = "Section heading to update or create")]
        section: Option<String>,

        #[arg(long, help = "Pattern or string to find")]
        find: Option<String>,

        #[arg(long, help = "Replacement string for --find")]
        replace: Option<String>,

        #[arg(
            value_name = "CONTENT",
            help = "New content (or '-' for STDIN). If omitted without flags, opens $EDITOR"
        )]
        content: Option<String>,
    },

    /// Append a timestamped log entry to a note (supports STDIN)
    Log {
        #[arg(help = "Note name or filename (with or without .md)")]
        note_name: String,

        #[arg(help = "Log entry content (pass '-' or omit to read from STDIN)")]
        entry: Option<String>,
    },

    /// Delete a note and remove it from search index
    #[command(alias = "delete", alias = "remove")]
    Rm {
        #[arg(help = "Note name or filename (with or without .md)")]
        note_name: String,
    },

    /// Synchronize Tantivy search index with markdown files on disk
    #[command(alias = "reindex")]
    Sync {
        #[arg(long, help = "Force rebuild index from scratch")]
        force: bool,
    },

    /// List notes in vault with optional tag filtering
    #[command(alias = "list")]
    Ls {
        #[arg(short, long, help = "Filter notes by tag")]
        tag: Option<String>,

        #[arg(
            long,
            default_value = "text",
            value_enum,
            help = "Output format: text or json"
        )]
        format: FormatChoice,
    },

    /// Search notes using regex or literal substring
    Grep {
        #[arg(help = "Pattern to search for")]
        pattern: String,

        #[arg(short, long, help = "Case-insensitive matching")]
        ignore_case: bool,

        #[arg(
            long,
            default_value = "text",
            value_enum,
            help = "Output format: text or json"
        )]
        format: FormatChoice,
    },

    /// Manage YAML frontmatter tags on a note
    Tag {
        #[command(subcommand)]
        action: TagAction,
    },

    /// List all unique tags across the vault and their occurrence count
    Tags {
        #[arg(
            long,
            default_value = "text",
            value_enum,
            help = "Output format: text or json"
        )]
        format: FormatChoice,
    },

    /// List orphaned notes with no outbound links and no backlinks
    Orphans,

    /// Prune notes matching a specific tag (e.g. scratchpads, temporary)
    Prune {
        #[arg(long, help = "Tag of notes to prune")]
        tag: String,
    },

    /// Print the absolute filesystem path to a note file
    Which {
        #[arg(help = "Note name or filename (with or without .md)")]
        note_name: String,
    },

    /// Print the absolute filesystem path to the vault directory
    Path,

    /// Count BPE tokens in a string, file, or STDIN ('-')
    Count {
        #[arg(help = "Text string, file path, or '-' for STDIN")]
        target: String,
    },

    /// Analyze outbound and backlink wikilinks for a note
    Links {
        #[arg(help = "Note name or filename (with or without .md)")]
        note_name: String,
    },

    /// Show vault statistics and Tantivy index status
    Status,
}

#[derive(Subcommand)]
enum TagAction {
    /// Add tags to a note
    Add {
        #[arg(help = "Note name")]
        note_name: String,
        #[arg(help = "Tags to add", required = true)]
        tags: Vec<String>,
    },
    /// Remove tags from a note
    Rm {
        #[arg(help = "Note name")]
        note_name: String,
        #[arg(help = "Tags to remove", required = true)]
        tags: Vec<String>,
    },
    /// List tags of a note
    List {
        #[arg(help = "Note name")]
        note_name: String,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum FormatChoice {
    Text,
    Json,
}

#[derive(Serialize)]
struct JsonGetResult {
    file: String,
    score: f32,
    tokens: usize,
    content: String,
}

#[derive(Serialize)]
struct JsonPackResult {
    total_tokens: usize,
    results: Vec<JsonGetResult>,
}

#[derive(Serialize)]
struct JsonReadResult {
    file: String,
    tokens: usize,
    content: String,
}

#[derive(Serialize)]
struct JsonListEntry {
    file: String,
    score: f32,
}

fn format_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.2} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

fn read_stdin_string() -> Result<String> {
    let mut buffer = String::new();
    std::io::stdin()
        .read_to_string(&mut buffer)
        .context("Failed to read from stdin")?;
    Ok(buffer)
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    match run_app(cli) {
        Ok(code) => code,
        Err(err) => {
            eprintln!("Error: {:#}", err);
            ExitCode::from(1)
        }
    }
}

fn run_app(cli: Cli) -> Result<ExitCode> {
    match cli.command {
        Commands::Init { path } => {
            let vault_path = match path {
                Some(p) => p,
                None => vault::resolve_vault_path(cli.vault.as_deref())?,
            };

            fs::create_dir_all(&vault_path).with_context(|| {
                format!("Failed to create vault path: {}", vault_path.display())
            })?;

            let index_path = index::get_index_path(&vault_path);
            let _ = index::open_or_create_index(&index_path)?;
            let _ = index::sync_all_notes(&vault_path, false)?;

            println!("Initialized eng vault at {}", vault_path.display());
            Ok(ExitCode::SUCCESS)
        }

        Commands::Get {
            query,
            tokens,
            top,
            list,
            section,
            format,
            hybrid,
            semantic,
        } => {
            let vault_path = vault::resolve_vault_path(cli.vault.as_deref())?;
            if !vault_path.exists() {
                bail!("Vault directory does not exist: {}", vault_path.display());
            }

            let search_limit = if list { 20 } else { top.max(1) * 3 };

            let results: Vec<index::SearchResult> = if semantic {
                let sem_hits = semantic::search_semantic(&vault_path, &query, search_limit)?;
                sem_hits
                    .into_iter()
                    .map(|s| {
                        let content = vault::find_note(&vault_path, &s.file)
                            .ok()
                            .flatten()
                            .and_then(|p| vault::read_note(&p).ok())
                            .map(|n| n.content)
                            .unwrap_or_default();
                        index::SearchResult {
                            file: s.file,
                            score: s.score,
                            content,
                        }
                    })
                    .collect()
            } else if hybrid {
                let bm25_hits = index::search_vault(&vault_path, &query, search_limit)?;
                let sem_hits = semantic::search_semantic(&vault_path, &query, search_limit)
                    .unwrap_or_default();
                let fused = semantic::hybrid_fuse(&bm25_hits, &sem_hits, 60.0);
                fused
                    .into_iter()
                    .take(search_limit)
                    .map(|(file, score)| {
                        let content = vault::find_note(&vault_path, &file)
                            .ok()
                            .flatten()
                            .and_then(|p| vault::read_note(&p).ok())
                            .map(|n| n.content)
                            .unwrap_or_default();
                        index::SearchResult {
                            file,
                            score,
                            content,
                        }
                    })
                    .collect()
            } else {
                index::search_vault(&vault_path, &query, search_limit)?
            };

            if results.is_empty() {
                return Ok(ExitCode::from(1));
            }

            if list {
                match format {
                    FormatChoice::Text => {
                        for (idx, r) in results.iter().enumerate() {
                            println!("{}. {} (score: {:.2})", idx + 1, r.file, r.score);
                        }
                    }
                    FormatChoice::Json => {
                        let entries: Vec<JsonListEntry> = results
                            .iter()
                            .map(|r| JsonListEntry {
                                file: r.file.clone(),
                                score: r.score,
                            })
                            .collect();
                        let json = serde_json::to_string(&entries)?;
                        println!("{}", json);
                    }
                }
                return Ok(ExitCode::SUCCESS);
            }

            // Single top hit
            if top <= 1 {
                let best_hit = &results[0];
                let note_path = vault_path.join(&best_hit.file);
                let note = if note_path.exists() {
                    vault::read_note(&note_path)?
                } else {
                    vault::Note {
                        filename: best_hit.file.clone(),
                        path: note_path,
                        frontmatter: vault::NoteFrontmatter::default(),
                        content: best_hit.content.clone(),
                        raw: best_hit.content.clone(),
                    }
                };

                let raw_content = if let Some(sec_name) = &section {
                    match vault::extract_section(&note.content, sec_name) {
                        Some(s) => s,
                        None => {
                            eprintln!(
                                "Section '{}' not found in note '{}'",
                                sec_name, best_hit.file
                            );
                            return Ok(ExitCode::from(1));
                        }
                    }
                } else {
                    note.content
                };

                let (trimmed_content, token_count) = if section.is_some() {
                    tokens::truncate_to_tokens(&raw_content, tokens)
                } else {
                    vault::extract_best_excerpt(&raw_content, &query, tokens)
                };

                match format {
                    FormatChoice::Text => {
                        println!("{}", trimmed_content);
                    }
                    FormatChoice::Json => {
                        let json_obj = JsonGetResult {
                            file: best_hit.file.clone(),
                            score: best_hit.score,
                            tokens: token_count,
                            content: trimmed_content,
                        };
                        let json_str = serde_json::to_string(&json_obj)?;
                        println!("{}", json_str);
                    }
                }
                return Ok(ExitCode::SUCCESS);
            }

            // Multi-hit packing (RAG pack) up to tokens budget
            let mut remaining_budget = tokens;
            let mut packed_results = Vec::new();
            let mut text_blocks = Vec::new();

            for hit in results.into_iter().take(top) {
                if remaining_budget == 0 {
                    break;
                }

                let note_path = vault_path.join(&hit.file);
                let note_content = if note_path.exists() {
                    vault::read_note(&note_path)?.content
                } else {
                    hit.content.clone()
                };

                let (excerpt, used_tokens) =
                    vault::extract_best_excerpt(&note_content, &query, remaining_budget);

                if !excerpt.trim().is_empty() && used_tokens > 0 {
                    remaining_budget = remaining_budget.saturating_sub(used_tokens);
                    text_blocks.push(format!("### {}\n{}", hit.file, excerpt));
                    packed_results.push(JsonGetResult {
                        file: hit.file,
                        score: hit.score,
                        tokens: used_tokens,
                        content: excerpt,
                    });
                }
            }

            match format {
                FormatChoice::Text => {
                    println!("{}", text_blocks.join("\n\n"));
                }
                FormatChoice::Json => {
                    let total_tokens: usize = packed_results.iter().map(|r| r.tokens).sum();
                    let pack = JsonPackResult {
                        total_tokens,
                        results: packed_results,
                    };
                    println!("{}", serde_json::to_string(&pack)?);
                }
            }

            Ok(ExitCode::SUCCESS)
        }

        Commands::Read {
            note_name,
            tokens,
            section,
            format,
        } => {
            let vault_path = vault::resolve_vault_path(cli.vault.as_deref())?;
            if !vault_path.exists() {
                bail!("Vault directory does not exist: {}", vault_path.display());
            }

            let note_path = match vault::find_note(&vault_path, &note_name)? {
                Some(p) => p,
                None => {
                    eprintln!("Note not found: {}", note_name);
                    return Ok(ExitCode::from(1));
                }
            };

            let note = vault::read_note(&note_path)?;

            let raw_content = if let Some(sec_name) = &section {
                match vault::extract_section(&note.content, sec_name) {
                    Some(s) => s,
                    None => {
                        eprintln!(
                            "Section '{}' not found in note '{}'",
                            sec_name, note.filename
                        );
                        return Ok(ExitCode::from(1));
                    }
                }
            } else {
                note.content
            };

            let (content, token_count) = if let Some(limit) = tokens {
                tokens::truncate_to_tokens(&raw_content, limit)
            } else {
                let count = tokens::count_tokens(&raw_content);
                (raw_content, count)
            };

            match format {
                FormatChoice::Text => {
                    println!("{}", content);
                }
                FormatChoice::Json => {
                    let json_obj = JsonReadResult {
                        file: note.filename,
                        tokens: token_count,
                        content,
                    };
                    println!("{}", serde_json::to_string(&json_obj)?);
                }
            }

            Ok(ExitCode::SUCCESS)
        }

        Commands::Push {
            content,
            title,
            tags,
            append,
        } => {
            let vault_path = vault::resolve_vault_path(cli.vault.as_deref())?;

            let body = if content == "-" {
                read_stdin_string()?
            } else {
                content
            };

            let tag_vec = tags.as_ref().map(|t| {
                t.split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<String>>()
            });

            let saved_filename = vault::save_note(
                &vault_path,
                title.as_deref(),
                &body,
                tag_vec.as_deref(),
                append,
            )?;

            let index_path = index::get_index_path(&vault_path);
            let (idx, fields) = index::open_or_create_index(&index_path)?;
            let saved_path = vault_path.join(&saved_filename);
            let saved_note = vault::read_note(&saved_path)?;
            index::index_single_note(&idx, &fields, &saved_note)?;

            if semantic::get_vectors_path(&vault_path).exists() {
                let _ = semantic::embed_single_note(&vault_path, &saved_note);
            }

            println!("OK: saved {}", saved_filename);
            Ok(ExitCode::SUCCESS)
        }

        Commands::Edit {
            note_name,
            section,
            find,
            replace,
            content,
        } => {
            let vault_path = vault::resolve_vault_path(cli.vault.as_deref())?;
            if !vault_path.exists() {
                fs::create_dir_all(&vault_path)?;
            }

            let (note_path, message) = if let Some(target_sec) = section {
                let text = match content {
                    Some(ref s) if s == "-" => read_stdin_string()?,
                    Some(s) => s,
                    None => {
                        if !std::io::stdin().is_terminal() {
                            read_stdin_string()?
                        } else {
                            bail!("No content provided for section '{}'. Pass content argument or pipe into STDIN.", target_sec);
                        }
                    }
                };
                let path = vault::edit_note_section(&vault_path, &note_name, &target_sec, &text)?;
                (
                    Some(path),
                    format!("OK: updated section '{}' in {}", target_sec, note_name),
                )
            } else if let Some(find_pat) = find {
                let rep = replace.unwrap_or_default();
                let (path, count) =
                    vault::edit_note_find_replace(&vault_path, &note_name, &find_pat, &rep)?;
                (
                    Some(path),
                    format!("OK: replaced {} occurrences in {}", count, note_name),
                )
            } else if let Some(body_text) = content {
                let text = if body_text == "-" {
                    read_stdin_string()?
                } else {
                    body_text
                };
                let path = vault::edit_note_body(&vault_path, &note_name, &text)?;
                (Some(path), format!("OK: updated body of {}", note_name))
            } else if !std::io::stdin().is_terminal() {
                let text = read_stdin_string()?;
                let path = vault::edit_note_body(&vault_path, &note_name, &text)?;
                (Some(path), format!("OK: updated body of {}", note_name))
            } else {
                // Interactive editor mode ($EDITOR / notepad)
                let resolved_path = match vault::find_note(&vault_path, &note_name)? {
                    Some(p) => p,
                    None => {
                        let filename = vault::sanitize_filename(&note_name);
                        vault_path.join(filename)
                    }
                };

                let modified = vault::launch_editor_for_note(&resolved_path)?;
                if modified {
                    (
                        Some(resolved_path),
                        format!("OK: updated {} and re-indexed", note_name),
                    )
                } else {
                    println!("OK: unchanged {}", note_name);
                    return Ok(ExitCode::SUCCESS);
                }
            };

            if let Some(p) = note_path {
                let index_path = index::get_index_path(&vault_path);
                if let Ok((idx, fields)) = index::open_or_create_index(&index_path) {
                    if let Ok(updated_note) = vault::read_note(&p) {
                        let _ = index::index_single_note(&idx, &fields, &updated_note);
                    }
                }

                if semantic::get_vectors_path(&vault_path).exists() {
                    if let Ok(updated_note) = vault::read_note(&p) {
                        let _ = semantic::embed_single_note(&vault_path, &updated_note);
                    }
                }
            }

            println!("{}", message);
            Ok(ExitCode::SUCCESS)
        }

        Commands::Embed { force } => {
            let vault_path = vault::resolve_vault_path(cli.vault.as_deref())?;
            if !vault_path.exists() {
                bail!("Vault directory does not exist: {}", vault_path.display());
            }

            let count = semantic::embed_vault(&vault_path, force)?;
            println!("OK: embedded {} notes", count);
            Ok(ExitCode::SUCCESS)
        }

        Commands::Log { note_name, entry } => {
            let vault_path = vault::resolve_vault_path(cli.vault.as_deref())?;

            let log_text = match entry {
                Some(ref s) if s == "-" => read_stdin_string()?,
                Some(s) => s,
                None => read_stdin_string()?,
            };

            let saved_filename = vault::append_log(&vault_path, &note_name, &log_text)?;

            let index_path = index::get_index_path(&vault_path);
            let (idx, fields) = index::open_or_create_index(&index_path)?;
            let saved_path = vault_path.join(&saved_filename);
            let saved_note = vault::read_note(&saved_path)?;
            index::index_single_note(&idx, &fields, &saved_note)?;

            if semantic::get_vectors_path(&vault_path).exists() {
                let _ = semantic::embed_single_note(&vault_path, &saved_note);
            }

            println!("OK: logged to {}", saved_filename);
            Ok(ExitCode::SUCCESS)
        }

        Commands::Rm { note_name } => {
            let vault_path = vault::resolve_vault_path(cli.vault.as_deref())?;
            if !vault_path.exists() {
                bail!("Vault directory does not exist: {}", vault_path.display());
            }

            let found = vault::find_note(&vault_path, &note_name)?;
            if found.is_none() {
                eprintln!("Note not found: {}", note_name);
                return Ok(ExitCode::from(1));
            }

            let deleted_filename = vault::delete_note(&vault_path, &note_name)?;

            let index_path = index::get_index_path(&vault_path);
            if let Ok((idx, fields)) = index::open_or_create_index(&index_path) {
                let _ = index::delete_single_note(&idx, &fields, &deleted_filename);
            }

            if semantic::get_vectors_path(&vault_path).exists() {
                if let Ok(mut store) = semantic::VectorStore::load(&vault_path) {
                    store.remove(&deleted_filename);
                    let _ = store.save(&vault_path);
                }
            }

            println!("OK: deleted {}", deleted_filename);
            Ok(ExitCode::SUCCESS)
        }

        Commands::Sync { force } => {
            let vault_path = vault::resolve_vault_path(cli.vault.as_deref())?;
            if !vault_path.exists() {
                bail!("Vault directory does not exist: {}", vault_path.display());
            }

            let count = index::sync_all_notes(&vault_path, force)?;
            println!("OK: indexed {} notes", count);
            Ok(ExitCode::SUCCESS)
        }

        Commands::Ls { tag, format } => {
            let vault_path = vault::resolve_vault_path(cli.vault.as_deref())?;
            if !vault_path.exists() {
                bail!("Vault directory does not exist: {}", vault_path.display());
            }

            let summaries = vault::list_notes_summary(&vault_path, tag.as_deref())?;

            match format {
                FormatChoice::Text => {
                    for s in summaries {
                        if s.tags.is_empty() {
                            println!("{}", s.file);
                        } else {
                            println!("{} [{}]", s.file, s.tags.join(", "));
                        }
                    }
                }
                FormatChoice::Json => {
                    println!("{}", serde_json::to_string(&summaries)?);
                }
            }

            Ok(ExitCode::SUCCESS)
        }

        Commands::Grep {
            pattern,
            ignore_case,
            format,
        } => {
            let vault_path = vault::resolve_vault_path(cli.vault.as_deref())?;
            if !vault_path.exists() {
                bail!("Vault directory does not exist: {}", vault_path.display());
            }

            let matches = vault::grep_notes(&vault_path, &pattern, ignore_case)?;
            if matches.is_empty() {
                return Ok(ExitCode::from(1));
            }

            match format {
                FormatChoice::Text => {
                    for m in matches {
                        println!("{}:{}: {}", m.file, m.line_number, m.line.trim());
                    }
                }
                FormatChoice::Json => {
                    println!("{}", serde_json::to_string(&matches)?);
                }
            }

            Ok(ExitCode::SUCCESS)
        }

        Commands::Tag { action } => {
            let vault_path = vault::resolve_vault_path(cli.vault.as_deref())?;
            if !vault_path.exists() {
                bail!("Vault directory does not exist: {}", vault_path.display());
            }

            match action {
                TagAction::Add { note_name, tags } => {
                    let (filename, updated) =
                        vault::update_tags(&vault_path, &note_name, &tags, &[])?;
                    let index_path = index::get_index_path(&vault_path);
                    let (idx, fields) = index::open_or_create_index(&index_path)?;
                    let saved_path = vault_path.join(&filename);
                    let saved_note = vault::read_note(&saved_path)?;
                    index::index_single_note(&idx, &fields, &saved_note)?;

                    println!("OK: tags for {} [{}]", filename, updated.join(", "));
                }
                TagAction::Rm { note_name, tags } => {
                    let (filename, updated) =
                        vault::update_tags(&vault_path, &note_name, &[], &tags)?;
                    let index_path = index::get_index_path(&vault_path);
                    let (idx, fields) = index::open_or_create_index(&index_path)?;
                    let saved_path = vault_path.join(&filename);
                    let saved_note = vault::read_note(&saved_path)?;
                    index::index_single_note(&idx, &fields, &saved_note)?;

                    println!("OK: tags for {} [{}]", filename, updated.join(", "));
                }
                TagAction::List { note_name } => {
                    let note_path = vault::find_note(&vault_path, &note_name)?
                        .with_context(|| format!("Note not found: {}", note_name))?;
                    let note = vault::read_note(&note_path)?;
                    println!("{}", note.frontmatter.tags.join(", "));
                }
            }

            Ok(ExitCode::SUCCESS)
        }

        Commands::Tags { format } => {
            let vault_path = vault::resolve_vault_path(cli.vault.as_deref())?;
            if !vault_path.exists() {
                bail!("Vault directory does not exist: {}", vault_path.display());
            }

            let tags = vault::collect_tags(&vault_path)?;

            match format {
                FormatChoice::Text => {
                    for t in tags {
                        println!("{} ({})", t.tag, t.count);
                    }
                }
                FormatChoice::Json => {
                    println!("{}", serde_json::to_string(&tags)?);
                }
            }

            Ok(ExitCode::SUCCESS)
        }

        Commands::Orphans => {
            let vault_path = vault::resolve_vault_path(cli.vault.as_deref())?;
            if !vault_path.exists() {
                bail!("Vault directory does not exist: {}", vault_path.display());
            }

            let orphans = vault::find_orphaned_notes(&vault_path)?;
            for note in orphans {
                println!("{}", note);
            }

            Ok(ExitCode::SUCCESS)
        }

        Commands::Prune { tag } => {
            let vault_path = vault::resolve_vault_path(cli.vault.as_deref())?;
            if !vault_path.exists() {
                bail!("Vault directory does not exist: {}", vault_path.display());
            }

            let pruned = vault::prune_notes_by_tag(&vault_path, &tag)?;
            if !pruned.is_empty() {
                let index_path = index::get_index_path(&vault_path);
                if let Ok((idx, fields)) = index::open_or_create_index(&index_path) {
                    for f in &pruned {
                        let _ = index::delete_single_note(&idx, &fields, f);
                    }
                }
            }

            println!("OK: pruned {} notes with tag '{}'", pruned.len(), tag);
            Ok(ExitCode::SUCCESS)
        }

        Commands::Which { note_name } => {
            let vault_path = vault::resolve_vault_path(cli.vault.as_deref())?;
            if !vault_path.exists() {
                bail!("Vault directory does not exist: {}", vault_path.display());
            }

            let note_path = match vault::find_note(&vault_path, &note_name)? {
                Some(p) => p,
                None => {
                    eprintln!("Note not found: {}", note_name);
                    return Ok(ExitCode::from(1));
                }
            };

            let canonical = note_path.canonicalize().unwrap_or(note_path);
            println!("{}", canonical.display());
            Ok(ExitCode::SUCCESS)
        }

        Commands::Path => {
            let vault_path = vault::resolve_vault_path(cli.vault.as_deref())?;
            let canonical = vault_path.canonicalize().unwrap_or(vault_path);
            println!("{}", canonical.display());
            Ok(ExitCode::SUCCESS)
        }

        Commands::Count { target } => {
            let text = if target == "-" {
                read_stdin_string()?
            } else {
                let target_path = Path::new(&target);
                if target_path.is_file() {
                    fs::read_to_string(target_path).with_context(|| {
                        format!("Failed to read file: {}", target_path.display())
                    })?
                } else if let Ok(vault_path) = vault::resolve_vault_path(cli.vault.as_deref()) {
                    if let Ok(Some(note_path)) = vault::find_note(&vault_path, &target) {
                        fs::read_to_string(&note_path)?
                    } else {
                        target
                    }
                } else {
                    target
                }
            };

            let count = tokens::count_tokens(&text);
            println!("{}", count);
            Ok(ExitCode::SUCCESS)
        }

        Commands::Links { note_name } => {
            let vault_path = vault::resolve_vault_path(cli.vault.as_deref())?;
            if !vault_path.exists() {
                bail!("Vault directory does not exist: {}", vault_path.display());
            }

            let links = vault::get_note_links(&vault_path, &note_name)?;

            println!("Outbound:");
            if links.outbound.is_empty() {
                println!("  (none)");
            } else {
                for target in &links.outbound {
                    println!("  - {}", target);
                }
            }

            println!("Backlinks:");
            if links.backlinks.is_empty() {
                println!("  (none)");
            } else {
                for source in &links.backlinks {
                    println!("  - {}", source);
                }
            }

            Ok(ExitCode::SUCCESS)
        }

        Commands::Status => {
            let vault_path = vault::resolve_vault_path(cli.vault.as_deref())?;
            let notes = vault::list_markdown_files(&vault_path).unwrap_or_default();
            let mut total_bytes: u64 = 0;
            for f in &notes {
                if let Ok(meta) = f.metadata() {
                    total_bytes += meta.len();
                }
            }

            let index_status = index::get_index_status(&vault_path).unwrap_or(index::IndexStatus {
                doc_count: 0,
                size_bytes: 0,
                ready: false,
            });

            let status_label = if index_status.ready {
                "ready"
            } else {
                "not initialized"
            };

            println!("Vault: {}", vault_path.display());
            println!("Notes: {} ({})", notes.len(), format_bytes(total_bytes));
            println!(
                "Indexed: {} (index size: {})",
                index_status.doc_count,
                format_bytes(index_status.size_bytes)
            );
            println!("Index status: {}", status_label);

            Ok(ExitCode::SUCCESS)
        }
    }
}
