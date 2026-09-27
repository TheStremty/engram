# engram

```text
  ___ _ __   __ _ _ __ __ _ _ __ ___  
 / _ \ '_ \ / _` | '__/ _` | '_ ` _ \ 
|  __/ | | | (_| | | | (_| | | | | | |
 \___|_| |_|\__, |_|  \__,_|_| |_| |_|
            |___/                     
```

[![CI](https://github.com/TheStremty/engram/actions/workflows/ci.yml/badge.svg)](https://github.com/TheStremty/engram/actions/workflows/ci.yml)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE)
[![Rust: 2021](https://img.shields.io/badge/rust-2021-orange.svg)](Cargo.toml)

> Ultralight, deterministic cache and long-term memory engine for terminal coding agents operating on a flat folder of Markdown files. Combines Tantivy full-text BM25 search with local neural vector embeddings via Reciprocal Rank Fusion (RRF).

---

## The "Agent Unix Tool" Philosophy

Unix tools were designed around a timeless maxim: *Write programs that do one thing and do it well. Write programs to work together. Write programs to handle text streams.*

However, terminal users today are no longer just humans. They are increasingly autonomous LLM coding agents (e.g. Antigravity, Claude Code, Cline, Cursor, Aider). Standard CLI tools designed for humans fail agentic workflows: they emit conversational fluff, wrap output in token-expensive ASCII borders, dump unbudgeted megabytes into prompt context windows, and lack concurrency safeguards.

`eng` is architected from the ground up as an **Agent Unix Tool**:

| Dimension | Human CLI | Agent Unix Tool (`eng`) |
| :--- | :--- | :--- |
| **Target Consumer** | Human eyes on a terminal emulator | LLM agent reasoning loop & context window |
| **Output Format** | Conversational chatter, ASCII boxes, ANSI spinners | Raw, deterministic, machine-parseable text |
| **Context Window** | Unbounded stdout (relies on manual `less`/paging) | Strict `--tokens` budget (BPE `cl100k_base`) with sentence boundaries |
| **Piping & STDIN** | Optional or secondary | First-class STDIN (`-`) for streaming test/compiler/diff streams |
| **Latency** | 500ms to 2500ms (Node/Python/Electron overhead) | **<6ms cold start** (compiled native Rust, zero daemon) |
| **Concurrency** | Single interactive user assumption | Exponential backoff lock retry for multi-agent teams |
| **Storage Vault** | Proprietary DB or heavy GUI client (Obsidian) | Flat Markdown + wikilinks (human & agent symbiosis) |

### Core Architectural Pillars:

1. **Token Economy as a First-Class Citizen**: Rather than blindly piping into `head -n 20` (which breaks sentences and ignores BPE token weights), `eng` truncates output strictly to a specified token budget using `tiktoken-rs` (`cl100k_base`), guaranteeing complete grammatical sentences whenever possible.
2. **Zero Conversational Noise**: No introductory pleasantries, no ANSI progress spinners, no decorative borders. Return codes are strictly deterministic (`0` on hit, `1` on miss/error).
3. **Deep Composability**: Native STDIN streaming (`cargo test 2>&1 | eng push - --title test-runs`) bypasses OS argument limits (`ARG_MAX`), while path introspection (`cat $(eng which note)`) integrates seamlessly with standard coreutils (`grep`, `awk`, `sed`).
4. **Subagent Concurrency Resilience**: Multiple autonomous agents running in parallel can write and log simultaneously. Embedded Tantivy index writers automatically resolve lock contention via exponential backoff.
5. **Human-Agent Symbiosis**: The memory vault is a plain directory of Markdown files with standard YAML frontmatter and `[[wikilinks]]`. A human developer can browse the graph in Obsidian or VS Code while an autonomous agent queries and updates it from the terminal at 5ms latency.

---

## Features

- **Hybrid Search (BM25 + Semantic RRF)**: Combines Tantivy lexical ranking with dense vector cosine similarity via Reciprocal Rank Fusion (`--hybrid`).
- **100% Local Vector Embeddings**: In-process ONNX runtime powered by `fastembed-rs` (`AllMiniLML6V2`, 384 dimensions). Zero external API calls, zero latency network hops, zero cloud costs.
- **Sub-6ms Cold Start**: Model weights load lazily on-demand only when semantic flags (`--hybrid`, `--semantic`, `eng embed`) are invoked. Lexical commands run instantly.
- **Accurate Token Budgeting**: Token counting and sentence-aware truncation powered by [tiktoken-rs](https://github.com/zurawiki/tiktoken-rs) (`cl100k_base`).
- **Granular Note Editing (`eng edit`)**: Interactively edit notes via `$EDITOR` or programmatically update specific sections (`--section`), perform in-place find & replace (`--find`/`--replace`), or stream updates through STDIN (`-`) while preserving existing YAML tags.
- **Multi-Document RAG Packing**: `eng get --top <K> --tokens <N>` packs context from multiple matching notes up to a strict token budget.
- **Wikilink Graph**: Fast extraction and bidirectional traversal of `[[Note Name]]` links + orphan detection (`eng orphans`).
- **Episodic Logging**: `eng log` appends timestamped entries (`[YYYY-MM-DD HH:MM:SS UTC]`).
- **YAML Frontmatter**: Native tag support (`tags: [...]`) with direct management (`eng tag add/rm`).
- **Unix Composability**: `eng which` (file path), `eng path` (vault path), and STDIN support (`-`) across commands.
- **Concurrency & Lock Retry**: Exponential backoff retry when multiple subagents access the index simultaneously.
- **Deterministic Exit Codes**: Code `0` on success, `1` on no hits or errors.
- **Auto Help**: Running `eng` without arguments displays complete usage instructions.

---

## Hybrid Search Architecture

Agents encounter two fundamentally different query patterns:
1. **Exact technical tokens**: Error codes (e.g. `E0277`), function names, compiler flags, and identifiers. Pure vector search frequently hallucinates or misses these.
2. **Conceptual and intent queries**: High-level descriptions like *"how do we handle concurrency locks"*. Pure BM25 fails if the note uses different terminology like *"thread safety and mutexes"*.

`eng` resolves this duality using **Reciprocal Rank Fusion (RRF)**:

$$\text{RRF}(d) = \frac{1}{60 + r_{\text{bm25}}(d)} + \frac{1}{60 + r_{\text{vector}}(d)}$$

- **Lexical Index**: Tantivy inverted index stored in `.eng_index/`.
- **Vector Store**: 384-dimensional dense vectors stored in `.eng_index/vectors.json`.
- **Automatic Sync**: Commands that mutate state (`push`, `edit`, `rm`) automatically update both the lexical index and the vector store in a single atomic pass.

---

## Vault Path Resolution

The vault location is resolved in the following priority order:
1. `--vault <PATH>` CLI parameter
2. `ENG_VAULT_PATH` environment variable
3. Default user directory: `~/eng_vault` (`%USERPROFILE%\eng_vault` on Windows)

---

## CLI Reference

### 1. `eng` (no arguments)
Running `eng` with no arguments automatically outputs help and usage instructions.

### 2. `eng init [PATH]`
Initializes the vault directory and the `.eng_index` directory.
```bash
eng init /path/to/my_vault
```

### 3. `eng push <CONTENT>`
Saves or appends a markdown note and immediately updates the search index and vector store.
- Pass `-` to read content from STDIN (`cat diff.txt | eng push - --title "pr-review"`).
- `--title <TITLE>`: Note filename / title. If omitted, generates slug from content.
- `--tags <TAGS>`: Comma-separated list of tags stored in YAML frontmatter.
- `--append`: Appends content to existing note instead of overwriting, merging tags.
```bash
eng push "Rust provides memory safety without garbage collection." --title "rust-safety" --tags "rust,systems"
cat log.txt | eng push - --title "deploy-log" --tags "ops"
```

### 4. `eng get <QUERY>`
Searches notes and returns the most relevant excerpt trimmed strictly to the token budget.
- `--tokens <N>`: Hard token limit (default: 200). Preserves sentence boundaries.
- `--hybrid`: Uses Hybrid Search (combines BM25 and vector embeddings via RRF).
- `--semantic`: Uses dense vector semantic search only.
- `--top <K>`: Number of top matching notes to combine within token budget (RAG packing).
- `--list`: Returns a numbered list of matching files and scores.
- `--section <NAME>`: Extracts a specific heading/section from top hit.
- `--format <text|json>`: Output format.
```bash
eng get "ownership" --tokens 150
eng get "database pooling" --hybrid --tokens 200
eng get "how to prevent memory leaks" --semantic --tokens 250
eng get "security" --top 3 --tokens 600
eng get "rust" --section "Ownership"
eng get "memory" --list
```

### 5. `eng embed`
Generates or updates local dense vector embeddings (`AllMiniLML6V2` via FastEmbed/ONNX) for all notes in the vault.
- `--force`: Re-generates embeddings for all notes from scratch.
```bash
eng embed
eng embed --force
```

### 6. `eng edit <NOTE_NAME> [CONTENT]`
Edits a note interactively in `$EDITOR` or programmatically with surgical precision (preserving YAML frontmatter/tags):
- `--section <HEADING>`: Replaces or creates a specific section without touching the rest of the note.
- `--find <PATTERN> --replace <STRING>`: In-place string substitution within the note body.
- STDIN pipe: Pass `-` (or pipe directly) to replace note body while preserving existing tags.
- Interactive mode: If omitted without flags, launches `$EDITOR` (or `notepad`/`nano`) and automatically re-indexes Tantivy and FastEmbed upon save and exit.
```bash
eng edit rust-safety                                  # Interactive editor
eng edit rust-safety --section "Ownership" "New text" # Granular section update
echo "New body" | eng edit rust-safety -              # Update body, preserve tags
eng edit rust-safety --find "old_fn" --replace "new_fn"
```

### 7. `eng read <NOTE_NAME>` (aliases: `cat`, `view`)
Reads a specific note directly by filename or title without BM25 search.
- `--tokens <N>`: Optional token budget truncation.
- `--section <NAME>`: Extracts a specific heading/section.
- `--format <text|json>`: Output format.
```bash
eng read rust-safety
eng read rust-safety --section "Ownership" --tokens 150
```

### 8. `eng log <NOTE_NAME> [ENTRY]`
Appends a timestamped entry (`- [YYYY-MM-DD HH:MM:SS UTC] <ENTRY>`) to a note.
Supports STDIN if entry is omitted or `-`.
```bash
eng log session "Fixed race condition in worker loop"
cargo test 2>&1 | eng log test-runs -
```

### 9. `eng rm <NOTE_NAME>` (aliases: `delete`, `remove`)
Deletes a note file from the vault and removes its document term from Tantivy index and vector store immediately.
```bash
eng rm scratchpad
```

### 10. `eng ls` (alias: `list`)
Lists notes in vault with optional tag filtering.
- `--tag <TAG>`: Filter notes containing this tag.
- `--format <text|json>`: Output format.
```bash
eng ls --tag rust
eng ls --format json
```

### 11. `eng grep <PATTERN>`
Searches notes using regex pattern or literal substring with optional `-i` (case-insensitive).
```bash
eng grep "E0277"
eng grep -i "failed to lock"
```

### 12. `eng tag <add|rm|list> <NOTE_NAME> [TAGS...]`
Modifies YAML frontmatter tags without touching note body content and reindexes immediately.
```bash
eng tag add rust-safety verified p1
eng tag rm rust-safety p1
eng tag list rust-safety
```

### 13. `eng tags`
Aggregates all unique tags across vault with count of associated notes.
```bash
eng tags
# Output:
# rust (3)
# systems (1)
```

### 14. `eng orphans`
Lists notes that have neither outbound wikilinks nor backlinks.
```bash
eng orphans
```

### 15. `eng prune --tag <TAG>`
Deletes all notes matching a tag (e.g. temporary notes or scratchpads) and cleans search index and vector store.
```bash
eng prune --tag temp
```

### 16. `eng which <NOTE_NAME>`
Prints the absolute filesystem path to a note file (ideal for `cat $(eng which note)`).
```bash
eng which rust-safety
```

### 17. `eng path`
Prints the absolute filesystem path to the current vault directory.
```bash
cd $(eng path)
```

### 18. `eng count <TEXT_OR_FILE>`
Counts BPE tokens (`cl100k_base`) in a string, note file, or STDIN (`-`).
```bash
eng count "Some prompt..."
eng count rust-safety
git diff | eng count -
```

### 19. `eng links <NOTE_NAME>`
Analyzes wikilinks `[[...]]` across the vault (outbound links and backlinks).
```bash
eng links rust-safety
```

### 20. `eng sync` (alias: `reindex`)
Re-synchronizes Tantivy search index with markdown files on disk. Cleans up orphaned index entries.
- `--force`: Rebuilds index completely from scratch.
```bash
eng sync --force
```

### 21. `eng status`
Displays vault note count, total size, indexed documents, and index status.
```bash
eng status
```

---

## Building and Testing

```bash
# Build optimized release binary
cargo build --release

# Run unit, integration, and benchmark tests
cargo test --all-targets

# Check code quality and formatting
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --check
```

---

## License

Dual-licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE](LICENSE) or http://opensource.org/licenses/MIT)

at your option.
