use anyhow::{Context, Result};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

#[derive(Debug, Serialize, Deserialize, Default, PartialEq, Clone)]
pub struct NoteFrontmatter {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Note {
    pub filename: String,
    pub path: PathBuf,
    pub frontmatter: NoteFrontmatter,
    pub content: String,
    pub raw: String,
}

static WIKILINK_RE: OnceLock<Regex> = OnceLock::new();
static SLUG_STRIP_RE: OnceLock<Regex> = OnceLock::new();

fn get_wikilink_re() -> &'static Regex {
    WIKILINK_RE.get_or_init(|| Regex::new(r"\[\[([^\]\|]+)(?:\|[^\]]+)?\]\]").unwrap())
}

fn get_slug_strip_re() -> &'static Regex {
    SLUG_STRIP_RE.get_or_init(|| Regex::new(r"[^a-zA-Z0-9\s-]").unwrap())
}

/// Resolves the vault path according to:
/// 1. Explicit argument / flag (--vault)
/// 2. Environment variable ENG_VAULT_PATH
/// 3. Default: ~/eng_vault
pub fn resolve_vault_path(explicit: Option<&Path>) -> Result<PathBuf> {
    if let Some(p) = explicit {
        return Ok(p.to_path_buf());
    }

    if let Ok(env_val) = std::env::var("ENG_VAULT_PATH") {
        if !env_val.trim().is_empty() {
            return Ok(PathBuf::from(env_val.trim()));
        }
    }

    let home = dirs::home_dir().context("Failed to determine user home directory")?;
    Ok(home.join("eng_vault"))
}

/// Generates a filename slug from note content.
pub fn generate_slug_from_content(content: &str) -> String {
    // Find first non-empty line
    let first_line = content
        .lines()
        .map(|l| l.trim())
        .find(|l| !l.is_empty() && !l.starts_with("---"))
        .unwrap_or("note");

    // Remove markdown heading markers (#), bullets, wikilink braces, quotes
    let cleaned = first_line.trim_start_matches(['#', '-', '*', '>']).trim();

    let stripped = get_slug_strip_re().replace_all(cleaned, "");
    let words: Vec<&str> = stripped.split_whitespace().take(5).collect();

    let mut slug = if words.is_empty() {
        "note".to_string()
    } else {
        words.join("-").to_lowercase()
    };

    if slug.len() > 40 {
        slug.truncate(40);
        slug = slug.trim_end_matches('-').to_string();
    }

    if slug.is_empty() {
        slug = "note".to_string();
    }

    format!("{}.md", slug)
}

/// Normalizes a note title into a safe `.md` filename.
pub fn sanitize_filename(title: &str) -> String {
    let mut trimmed = title.trim();
    if let Some(stripped) = trimmed.strip_suffix(".md") {
        trimmed = stripped;
    }

    // Replace invalid path characters with '-'
    let sanitized: String = trimmed
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '-',
            _ => c,
        })
        .collect();

    let safe = sanitized.trim_matches(|c: char| c == '-' || c == ' ' || c == '.');
    let base = if safe.is_empty() { "note" } else { safe };
    format!("{}.md", base)
}

/// Parses frontmatter and body from raw Markdown content.
pub fn parse_frontmatter_and_content(raw: &str) -> (NoteFrontmatter, String) {
    let trimmed = raw.trim_start();
    if !trimmed.starts_with("---") {
        return (NoteFrontmatter::default(), raw.to_string());
    }

    let mut lines = trimmed.lines();
    // First line must be "---"
    let first = lines.next().map(|l| l.trim());
    if first != Some("---") {
        return (NoteFrontmatter::default(), raw.to_string());
    }

    let mut yaml_lines = Vec::new();
    let mut found_closing = false;
    let mut body_lines = Vec::new();

    for line in lines {
        if !found_closing {
            if line.trim() == "---" {
                found_closing = true;
            } else {
                yaml_lines.push(line);
            }
        } else {
            body_lines.push(line);
        }
    }

    if found_closing {
        let yaml_str = yaml_lines.join("\n");
        let body = body_lines.join("\n").trim().to_string();
        if let Ok(frontmatter) = serde_yaml::from_str::<NoteFrontmatter>(&yaml_str) {
            return (frontmatter, body);
        }
    }

    (NoteFrontmatter::default(), raw.to_string())
}

/// Renders NoteFrontmatter + body into raw Markdown.
pub fn render_note(frontmatter: &NoteFrontmatter, content: &str) -> String {
    let has_tags = !frontmatter.tags.is_empty();
    if !has_tags {
        return content.to_string();
    }

    let yaml = serde_yaml::to_string(frontmatter).unwrap_or_default();
    format!("---\n{}---\n\n{}", yaml, content.trim())
}

/// Extracts wikilinks `[[Target]]` from markdown text.
pub fn extract_wikilinks(text: &str) -> Vec<String> {
    let re = get_wikilink_re();
    let mut links = Vec::new();
    let mut seen = HashSet::new();

    for cap in re.captures_iter(text) {
        if let Some(target) = cap.get(1) {
            let t = target.as_str().trim();
            let norm = if let Some(stripped) = t.strip_suffix(".md") {
                stripped.trim().to_string()
            } else {
                t.to_string()
            };
            if !norm.is_empty() && seen.insert(norm.to_lowercase()) {
                links.push(norm);
            }
        }
    }
    links
}

/// Extracts a section matching a heading name from markdown content.
pub fn extract_section(markdown: &str, target_heading: &str) -> Option<String> {
    let target = target_heading
        .trim()
        .trim_start_matches('#')
        .trim()
        .to_lowercase();
    let mut in_section = false;
    let mut section_level = 0;
    let mut extracted_lines = Vec::new();

    for line in markdown.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with('#') {
            let heading_level = trimmed.chars().take_while(|&c| c == '#').count();
            let heading_text = trimmed[heading_level..].trim();
            let h_lower = heading_text.to_lowercase();

            if in_section {
                // If we encounter a heading of equal or higher level (fewer or equal '#'s), section ends
                if heading_level <= section_level {
                    break;
                } else {
                    extracted_lines.push(line);
                }
            } else if h_lower == target || h_lower.contains(&target) {
                in_section = true;
                section_level = heading_level;
                // Include the heading itself in the extracted section
                extracted_lines.push(line);
            }
        } else if in_section {
            extracted_lines.push(line);
        }
    }

    if in_section && !extracted_lines.is_empty() {
        Some(extracted_lines.join("\n").trim().to_string())
    } else {
        None
    }
}

/// Updates an existing section in markdown, or appends a new section if not found.
pub fn upsert_section(markdown: &str, target_heading: &str, new_content: &str) -> String {
    let target = target_heading
        .trim()
        .trim_start_matches('#')
        .trim()
        .to_lowercase();
    let lines: Vec<&str> = markdown.lines().collect();
    let mut found_start = None;
    let mut found_end = None;
    let mut section_level = 2; // Default to ## if creating
    let mut matched_heading_line = String::new();

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.starts_with('#') {
            let heading_level = trimmed.chars().take_while(|&c| c == '#').count();
            let heading_text = trimmed[heading_level..].trim();
            let h_lower = heading_text.to_lowercase();

            if found_start.is_some() {
                if heading_level <= section_level {
                    found_end = Some(i);
                    break;
                }
            } else if h_lower == target || h_lower.contains(&target) {
                found_start = Some(i);
                section_level = heading_level;
                matched_heading_line = line.to_string();
            }
        }
    }

    let replacement_block = if new_content.trim_start().starts_with('#') {
        new_content.trim().to_string()
    } else if found_start.is_some() {
        format!(
            "{}\n\n{}",
            matched_heading_line.trim_end(),
            new_content.trim()
        )
    } else {
        format!(
            "## {}\n\n{}",
            target_heading.trim().trim_start_matches('#').trim(),
            new_content.trim()
        )
    };

    if let Some(start_idx) = found_start {
        let end_idx = found_end.unwrap_or(lines.len());
        let mut result_parts = Vec::new();
        if start_idx > 0 {
            result_parts.push(lines[..start_idx].join("\n").trim_end().to_string());
        }
        result_parts.push(replacement_block);
        if end_idx < lines.len() {
            result_parts.push(lines[end_idx..].join("\n").trim_start().to_string());
        }
        result_parts.join("\n\n").trim().to_string()
    } else {
        let trimmed = markdown.trim();
        if trimmed.is_empty() {
            replacement_block
        } else {
            format!("{}\n\n{}", trimmed, replacement_block)
        }
    }
}

/// Extracts the most relevant excerpt matching `query` within `max_tokens`.
pub fn extract_best_excerpt(content: &str, query: &str, max_tokens: usize) -> (String, usize) {
    let (full_trunc, full_tokens) = crate::tokens::truncate_to_tokens(content, max_tokens);
    if crate::tokens::count_tokens(content) <= max_tokens {
        return (full_trunc, full_tokens);
    }

    let query_terms: Vec<String> = query
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() > 1)
        .map(|w| w.to_lowercase())
        .collect();

    if query_terms.is_empty() {
        return crate::tokens::truncate_to_tokens(content, max_tokens);
    }

    // Split content by paragraphs
    let paragraphs: Vec<&str> = content.split("\n\n").collect();
    let mut best_idx = 0;
    let mut best_score = 0;

    for (i, p) in paragraphs.iter().enumerate() {
        let p_lower = p.to_lowercase();
        let mut score = 0;
        for term in &query_terms {
            if p_lower.contains(term) {
                score += 1;
            }
        }
        if score > best_score {
            best_score = score;
            best_idx = i;
        }
    }

    let candidate = paragraphs[best_idx..].join("\n\n");
    crate::tokens::truncate_to_tokens(&candidate, max_tokens)
}

/// Lists all markdown files in vault directory (excluding hidden files/directories like .eng_index).
pub fn list_markdown_files(vault_path: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    if !vault_path.exists() {
        return Ok(files);
    }

    for entry in fs::read_dir(vault_path).context("Failed to read vault directory")? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if !name.starts_with('.') && name.ends_with(".md") {
                    files.push(path);
                }
            }
        }
    }
    files.sort();
    Ok(files)
}

/// Reads a note from the vault.
pub fn read_note(path: &Path) -> Result<Note> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("Failed to read note file: {}", path.display()))?;
    let (frontmatter, content) = parse_frontmatter_and_content(&raw);
    let filename = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_string();

    Ok(Note {
        filename,
        path: path.to_path_buf(),
        frontmatter,
        content,
        raw,
    })
}

/// Finds a note by name (case-insensitive, with or without .md).
pub fn find_note(vault_path: &Path, note_name: &str) -> Result<Option<PathBuf>> {
    let target_norm = if let Some(stripped) = note_name.strip_suffix(".md") {
        stripped.trim().to_lowercase()
    } else {
        note_name.trim().to_lowercase()
    };

    for file in list_markdown_files(vault_path)? {
        if let Some(stem) = file.file_stem().and_then(|s| s.to_str()) {
            if stem.to_lowercase() == target_norm {
                return Ok(Some(file));
            }
        }
    }
    Ok(None)
}

/// Saves or appends a note to the vault. Returns the saved filename.
pub fn save_note(
    vault_path: &Path,
    title: Option<&str>,
    content: &str,
    tags: Option<&[String]>,
    append: bool,
) -> Result<String> {
    fs::create_dir_all(vault_path).context("Failed to create vault directory")?;

    let filename = match title {
        Some(t) => sanitize_filename(t),
        None => generate_slug_from_content(content),
    };

    let file_path = vault_path.join(&filename);

    let (final_frontmatter, final_body) = if append && file_path.exists() {
        let existing = read_note(&file_path)?;
        let mut fm = existing.frontmatter;

        if let Some(new_tags) = tags {
            let mut tag_set: HashSet<String> = fm.tags.into_iter().collect();
            for t in new_tags {
                if !t.trim().is_empty() {
                    tag_set.insert(t.trim().to_string());
                }
            }
            let mut merged_tags: Vec<String> = tag_set.into_iter().collect();
            merged_tags.sort();
            fm.tags = merged_tags;
        }

        let appended_body = if existing.content.trim().is_empty() {
            content.trim().to_string()
        } else {
            format!("{}\n\n{}", existing.content.trim(), content.trim())
        };

        (fm, appended_body)
    } else {
        let mut fm = NoteFrontmatter::default();
        if let Some(new_tags) = tags {
            fm.tags = new_tags
                .iter()
                .filter(|t| !t.trim().is_empty())
                .map(|t| t.trim().to_string())
                .collect();
        } else if file_path.exists() {
            if let Ok(existing) = read_note(&file_path) {
                fm = existing.frontmatter;
            }
        }
        (fm, content.trim().to_string())
    };

    let rendered = render_note(&final_frontmatter, &final_body);
    fs::write(&file_path, rendered)
        .with_context(|| format!("Failed to write note: {}", file_path.display()))?;

    Ok(filename)
}

/// Edits or inserts a section in an existing or new note. Returns the note's PathBuf.
pub fn edit_note_section(
    vault_path: &Path,
    note_name: &str,
    target_heading: &str,
    new_content: &str,
) -> Result<PathBuf> {
    let note_path = match find_note(vault_path, note_name)? {
        Some(p) => p,
        None => {
            let filename = sanitize_filename(note_name);
            vault_path.join(filename)
        }
    };

    let raw = if note_path.exists() {
        fs::read_to_string(&note_path)?
    } else {
        String::new()
    };

    let (fm, body) = parse_frontmatter_and_content(&raw);
    let new_body = upsert_section(&body, target_heading, new_content);
    let rendered = render_note(&fm, &new_body);
    fs::write(&note_path, rendered)
        .with_context(|| format!("Failed to write note: {}", note_path.display()))?;
    Ok(note_path)
}

/// Updates the body of a note, preserving its existing frontmatter tags unless new frontmatter is provided.
pub fn edit_note_body(
    vault_path: &Path,
    note_name: &str,
    new_body_or_raw: &str,
) -> Result<PathBuf> {
    let note_path = match find_note(vault_path, note_name)? {
        Some(p) => p,
        None => {
            let filename = sanitize_filename(note_name);
            vault_path.join(filename)
        }
    };

    let existing_raw = if note_path.exists() {
        fs::read_to_string(&note_path)?
    } else {
        String::new()
    };

    let (existing_fm, _) = parse_frontmatter_and_content(&existing_raw);
    let (new_fm, pure_body) = parse_frontmatter_and_content(new_body_or_raw);

    let final_fm = if !new_fm.tags.is_empty() {
        new_fm
    } else {
        existing_fm
    };

    let rendered = render_note(&final_fm, &pure_body);
    fs::write(&note_path, rendered)
        .with_context(|| format!("Failed to write note: {}", note_path.display()))?;
    Ok(note_path)
}

/// Finds and replaces occurrences of a string within the body of a note.
pub fn edit_note_find_replace(
    vault_path: &Path,
    note_name: &str,
    find: &str,
    replace: &str,
) -> Result<(PathBuf, usize)> {
    let note_path = find_note(vault_path, note_name)?
        .ok_or_else(|| anyhow::anyhow!("Note not found: {}", note_name))?;

    let raw = fs::read_to_string(&note_path)?;
    let (fm, body) = parse_frontmatter_and_content(&raw);
    let count = body.matches(find).count();
    if count == 0 {
        return Ok((note_path, 0));
    }
    let new_body = body.replace(find, replace);
    let rendered = render_note(&fm, &new_body);
    fs::write(&note_path, rendered)
        .with_context(|| format!("Failed to write note: {}", note_path.display()))?;
    Ok((note_path, count))
}

/// Launches the interactive editor ($VISUAL / $EDITOR / notepad) for the note.
/// Returns true if the file was modified, false otherwise.
pub fn launch_editor_for_note(note_path: &Path) -> Result<bool> {
    let initial_content = if note_path.exists() {
        fs::read_to_string(note_path).unwrap_or_default()
    } else {
        if let Some(parent) = note_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(note_path, "")?;
        String::new()
    };

    let editor = std::env::var("VISUAL")
        .or_else(|_| std::env::var("EDITOR"))
        .unwrap_or_else(|_| {
            if cfg!(windows) {
                "notepad.exe".to_string()
            } else {
                "nano".to_string()
            }
        });

    let status = std::process::Command::new(&editor)
        .arg(note_path)
        .status()
        .with_context(|| format!("Failed to launch editor '{}'", editor))?;

    if !status.success() {
        anyhow::bail!(
            "Editor '{}' exited with non-zero status: {}",
            editor,
            status
        );
    }

    let final_content = fs::read_to_string(note_path).unwrap_or_default();
    Ok(final_content != initial_content)
}

pub struct NoteLinks {
    pub outbound: Vec<String>,
    pub backlinks: Vec<String>,
}

/// Finds outbound links and backlinks for a given note name.
pub fn get_note_links(vault_path: &Path, note_name: &str) -> Result<NoteLinks> {
    let note_path = find_note(vault_path, note_name)?
        .with_context(|| format!("Note not found: {}", note_name))?;

    let current_stem = note_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_lowercase();

    let note = read_note(&note_path)?;
    let outbound = extract_wikilinks(&note.raw);

    let mut backlinks = Vec::new();
    for file in list_markdown_files(vault_path)? {
        if file == note_path {
            continue;
        }
        let other_note = read_note(&file)?;
        let links = extract_wikilinks(&other_note.raw);
        for link in links {
            if link.to_lowercase() == current_stem {
                let other_filename = file
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_string();
                backlinks.push(other_filename);
                break;
            }
        }
    }

    backlinks.sort();
    Ok(NoteLinks {
        outbound,
        backlinks,
    })
}

/// Deletes a note file from the vault. Returns the deleted filename.
pub fn delete_note(vault_path: &Path, note_name: &str) -> Result<String> {
    let note_path = find_note(vault_path, note_name)?
        .with_context(|| format!("Note not found: {}", note_name))?;

    let filename = note_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_string();

    fs::remove_file(&note_path)
        .with_context(|| format!("Failed to delete note file: {}", note_path.display()))?;

    Ok(filename)
}

#[derive(Debug, Serialize, Clone)]
pub struct NoteSummary {
    pub file: String,
    pub tags: Vec<String>,
    pub tokens: usize,
}

/// Lists notes in the vault with their tags and token counts, optionally filtered by tag.
pub fn list_notes_summary(vault_path: &Path, tag_filter: Option<&str>) -> Result<Vec<NoteSummary>> {
    let files = list_markdown_files(vault_path)?;
    let mut summaries = Vec::new();

    for file in files {
        if let Ok(note) = read_note(&file) {
            if let Some(filter) = tag_filter {
                let matches_tag = note
                    .frontmatter
                    .tags
                    .iter()
                    .any(|t| t.eq_ignore_ascii_case(filter));
                if !matches_tag {
                    continue;
                }
            }

            let token_count = crate::tokens::count_tokens(&note.content);
            summaries.push(NoteSummary {
                file: note.filename,
                tags: note.frontmatter.tags,
                tokens: token_count,
            });
        }
    }

    summaries.sort_by(|a, b| a.file.cmp(&b.file));
    Ok(summaries)
}

#[derive(Debug, Serialize, Clone)]
pub struct TagSummary {
    pub tag: String,
    pub count: usize,
}

/// Collects all unique tags with count of notes per tag across the vault.
pub fn collect_tags(vault_path: &Path) -> Result<Vec<TagSummary>> {
    let files = list_markdown_files(vault_path)?;
    let mut tag_map = std::collections::HashMap::new();

    for file in files {
        if let Ok(note) = read_note(&file) {
            for tag in note.frontmatter.tags {
                *tag_map.entry(tag).or_insert(0) += 1;
            }
        }
    }

    let mut list: Vec<TagSummary> = tag_map
        .into_iter()
        .map(|(tag, count)| TagSummary { tag, count })
        .collect();

    // Sort by count descending, then alphabetically by tag name
    list.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.tag.cmp(&b.tag)));
    Ok(list)
}

/// Appends a timestamped log entry to a note.
pub fn append_log(vault_path: &Path, note_name: &str, entry: &str) -> Result<String> {
    fs::create_dir_all(vault_path).context("Failed to create vault directory")?;

    let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC");
    let log_line = format!("- [{}] {}", now, entry.trim());

    let note_file = find_note(vault_path, note_name)?;
    let (file_path, filename) = match note_file {
        Some(p) => {
            let fname = p
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();
            (p, fname)
        }
        None => {
            let fname = sanitize_filename(note_name);
            let p = vault_path.join(&fname);
            (p, fname)
        }
    };

    if file_path.exists() {
        let existing = read_note(&file_path)?;
        let updated_content = if existing.content.trim().is_empty() {
            log_line
        } else {
            format!("{}\n{}", existing.content.trim(), log_line)
        };
        let rendered = render_note(&existing.frontmatter, &updated_content);
        fs::write(&file_path, rendered)?;
    } else {
        let title = note_name.trim_end_matches(".md");
        let body = format!("# {}\n\n{}\n", title, log_line);
        fs::write(&file_path, body)?;
    }

    Ok(filename)
}

/// Adds and/or removes tags on a note without altering note body content.
pub fn update_tags(
    vault_path: &Path,
    note_name: &str,
    add_tags: &[String],
    rm_tags: &[String],
) -> Result<(String, Vec<String>)> {
    let note_path = find_note(vault_path, note_name)?
        .with_context(|| format!("Note not found: {}", note_name))?;

    let note = read_note(&note_path)?;
    let mut fm = note.frontmatter;

    // Remove tags
    fm.tags
        .retain(|t| !rm_tags.iter().any(|r| r.eq_ignore_ascii_case(t)));

    // Add tags
    for add in add_tags {
        let trimmed = add.trim();
        if !trimmed.is_empty() && !fm.tags.iter().any(|t| t.eq_ignore_ascii_case(trimmed)) {
            fm.tags.push(trimmed.to_string());
        }
    }
    fm.tags.sort();

    let rendered = render_note(&fm, &note.content);
    fs::write(&note_path, rendered)?;

    Ok((note.filename, fm.tags))
}

/// Finds notes that have neither outbound links nor backlinks.
pub fn find_orphaned_notes(vault_path: &Path) -> Result<Vec<String>> {
    let files = list_markdown_files(vault_path)?;
    let mut orphans = Vec::new();

    for file in &files {
        let note = read_note(file)?;
        let stem = file.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        let links = get_note_links(vault_path, stem)?;
        if links.outbound.is_empty() && links.backlinks.is_empty() {
            orphans.push(note.filename);
        }
    }

    orphans.sort();
    Ok(orphans)
}

/// Prunes (deletes) all notes containing a given tag. Returns deleted filenames.
pub fn prune_notes_by_tag(vault_path: &Path, tag: &str) -> Result<Vec<String>> {
    let files = list_markdown_files(vault_path)?;
    let mut deleted = Vec::new();

    for file in files {
        if let Ok(note) = read_note(&file) {
            if note
                .frontmatter
                .tags
                .iter()
                .any(|t| t.eq_ignore_ascii_case(tag))
            {
                let _ = fs::remove_file(&file);
                deleted.push(note.filename);
            }
        }
    }

    deleted.sort();
    Ok(deleted)
}

#[derive(Debug, Serialize, Clone)]
pub struct GrepMatch {
    pub file: String,
    pub line_number: usize,
    pub line: String,
}

/// Searches notes for a regex pattern.
pub fn grep_notes(vault_path: &Path, pattern: &str, ignore_case: bool) -> Result<Vec<GrepMatch>> {
    let files = list_markdown_files(vault_path)?;
    let re = regex::RegexBuilder::new(pattern)
        .case_insensitive(ignore_case)
        .build()
        .with_context(|| format!("Invalid regex pattern: {}", pattern))?;

    let mut matches = Vec::new();

    for file in files {
        if let Ok(raw) = fs::read_to_string(&file) {
            let fname = file
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();
            for (idx, line) in raw.lines().enumerate() {
                if re.is_match(line) {
                    matches.push(GrepMatch {
                        file: fname.clone(),
                        line_number: idx + 1,
                        line: line.to_string(),
                    });
                }
            }
        }
    }

    Ok(matches)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slug_generation() {
        let content = "# Project Architecture Overview\nThis is a test description.";
        let slug = generate_slug_from_content(content);
        assert_eq!(slug, "project-architecture-overview.md");

        let content2 = "Just a short note without headings";
        let slug2 = generate_slug_from_content(content2);
        assert_eq!(slug2, "just-a-short-note-without.md");
    }

    #[test]
    fn test_sanitize_filename() {
        assert_eq!(sanitize_filename("My Note"), "My Note.md");
        assert_eq!(sanitize_filename("test/note:v1.md"), "test-note-v1.md");
    }

    #[test]
    fn test_frontmatter_parsing_and_rendering() {
        let raw = "---\ntags:\n  - rust\n  - cli\n---\n\n# Header\nNote content here.";
        let (fm, body) = parse_frontmatter_and_content(raw);
        assert_eq!(fm.tags, vec!["rust", "cli"]);
        assert_eq!(body, "# Header\nNote content here.");

        let rendered = render_note(&fm, &body);
        let (fm2, body2) = parse_frontmatter_and_content(&rendered);
        assert_eq!(fm2.tags, fm.tags);
        assert_eq!(body2, body);
    }

    #[test]
    fn test_wikilinks_extraction() {
        let text = "Refer to [[Architecture]] and [[API Guide|API]] also [[Architecture.md]].";
        let links = extract_wikilinks(text);
        assert_eq!(links, vec!["Architecture", "API Guide"]);
    }

    #[test]
    fn test_extract_section() {
        let md = "# Title\n\nIntro\n\n## Section A\nContent of section A\nMore lines\n\n## Section B\nContent B";
        let sec = extract_section(md, "Section A").unwrap();
        assert!(sec.contains("## Section A"));
        assert!(sec.contains("Content of section A"));
        assert!(!sec.contains("## Section B"));
    }

    #[test]
    fn test_upsert_section_update() {
        let md = "# Title\n\nIntro\n\n## Section A\nOld content\n\n## Section B\nContent B";
        let updated = upsert_section(md, "Section A", "Brand new content A");
        assert!(updated.contains("## Section A\n\nBrand new content A"));
        assert!(updated.contains("## Section B\nContent B"));
        assert!(!updated.contains("Old content"));
    }

    #[test]
    fn test_upsert_section_append() {
        let md = "# Title\n\nIntro\n\n## Section A\nContent A";
        let updated = upsert_section(md, "Section New", "Fresh content");
        assert!(updated.contains("## Section New\n\nFresh content"));
        assert!(updated.contains("## Section A\nContent A"));
    }

    #[test]
    fn test_edit_note_helpers() {
        let temp = tempfile::tempdir().unwrap();
        let vault = temp.path();

        // Create initial note
        save_note(
            vault,
            Some("test-note"),
            "# Initial\n\n## Sec 1\nHello world",
            Some(&["tag1".into()]),
            false,
        )
        .unwrap();

        // Edit section
        edit_note_section(vault, "test-note", "Sec 1", "Updated greetings").unwrap();
        let note = read_note(&vault.join("test-note.md")).unwrap();
        assert_eq!(note.frontmatter.tags, vec!["tag1"]);
        assert!(note.content.contains("Updated greetings"));

        // Find replace
        let (_, count) =
            edit_note_find_replace(vault, "test-note", "greetings", "universe").unwrap();
        assert_eq!(count, 1);
        let note = read_note(&vault.join("test-note.md")).unwrap();
        assert!(note.content.contains("Updated universe"));

        // Edit body preserving frontmatter
        edit_note_body(vault, "test-note", "# Clean Body\nAll new content here").unwrap();
        let note = read_note(&vault.join("test-note.md")).unwrap();
        assert_eq!(note.frontmatter.tags, vec!["tag1"]);
        assert_eq!(note.content, "# Clean Body\nAll new content here");
    }
}
