use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Instant;
use tempfile::tempdir;

#[test]
fn run_battle_and_benchmark() {
    let dir = tempdir().expect("Failed to create temp dir for benchmark");
    let vault_path = dir.path().to_str().unwrap();
    let exe = env!("CARGO_BIN_EXE_eng");

    println!("\n========================================================");
    println!("        ENG BATTLE TEST & BENCHMARK SUITE");
    println!("========================================================");

    // 1. Measure CLI Cold Startup
    let start = Instant::now();
    for _ in 0..10 {
        let _ = Command::new(exe).arg("--version").output().unwrap();
    }
    let startup_avg = start.elapsed() / 10;
    println!(
        "1. Cold CLI Startup (average over 10 runs): {:?}",
        startup_avg
    );

    // 2. Initialize Vault
    let start = Instant::now();
    let init_out = Command::new(exe)
        .arg("init")
        .arg(vault_path)
        .output()
        .unwrap();
    assert!(init_out.status.success());
    println!("2. eng init latency: {:?}", start.elapsed());

    // 3. Battle Ingestion: 60 realistic technical notes with tags, headings, and wikilinks
    println!("\n3. Ingesting 60 technical notes with wikilinks & frontmatter...");
    let topics = [
        (
            "auth-jwt",
            "Authentication using JWT tokens and asymmetric RSA keys.",
            "security,auth",
            "[[crypto-vault]]",
        ),
        (
            "crypto-vault",
            "Cryptographic key derivation and AES-256-GCM envelope encryption.",
            "security,crypto",
            "[[key-management]]",
        ),
        (
            "key-management",
            "KMS rotation policy and HSM hardware security modules.",
            "security,kms",
            "[[auth-jwt]]",
        ),
        (
            "database-postgres",
            "PostgreSQL connection pooling with PgBouncer and replica lag.",
            "database,postgres",
            "[[cache-redis]]",
        ),
        (
            "cache-redis",
            "Redis caching layer with LRU eviction and memory optimization.",
            "cache,redis",
            "[[database-postgres]]",
        ),
        (
            "tantivy-indexing",
            "Tantivy inverted index schema, term dictionary and BM25 scoring.",
            "search,tantivy",
            "[[token-budgeting]]",
        ),
        (
            "token-budgeting",
            "Tiktoken BPE token counting with sentence preservation algorithms.",
            "tokens,llm",
            "[[tantivy-indexing]]",
        ),
        (
            "concurrency-locks",
            "File locking mechanisms using exponential backoff retry.",
            "concurrency,os",
            "[[thread-pool]]",
        ),
        (
            "thread-pool",
            "Rayon thread pool work stealing and CPU core saturation.",
            "concurrency,rayon",
            "[[concurrency-locks]]",
        ),
        (
            "error-handling",
            "Anyhow and thiserror context chaining in Rust CLI tools.",
            "rust,errors",
            "[[logging-tracing]]",
        ),
        (
            "logging-tracing",
            "Structured logging with tracing crate and ISO 8601 timestamps.",
            "logging,ops",
            "[[error-handling]]",
        ),
        (
            "memory-safety",
            "Rust borrow checker prevents data races and use-after-free bugs.",
            "rust,safety",
            "[[zero-cost-abstractions]]",
        ),
        (
            "zero-cost-abstractions",
            "Iterators and monomorphization optimize down to tight assembly.",
            "rust,perf",
            "[[memory-safety]]",
        ),
        (
            "rate-limiting",
            "Token bucket and leaky bucket algorithms in distributed gateways.",
            "network,api",
            "[[cache-redis]]",
        ),
        (
            "api-gateway",
            "Reverse proxy routing with TLS termination and gzip compression.",
            "network,gateway",
            "[[rate-limiting]]",
        ),
    ];

    let start_ingest = Instant::now();
    let mut doc_count = 0;
    for round in 1..=4 {
        for (stem, body, tags, link) in &topics {
            let title = format!("{}-v{}", stem, round);
            let content = format!(
                "# {}\n\n{}\n\n## Deep Dive\nDetailed explanation of {} architecture.\nRefer to {} for foundational concepts.\n\n## References\nSee also {}.",
                title, body, stem, link, link
            );

            let status = Command::new(exe)
                .arg("--vault")
                .arg(vault_path)
                .arg("push")
                .arg(&content)
                .arg("--title")
                .arg(&title)
                .arg("--tags")
                .arg(tags)
                .output()
                .unwrap();
            assert!(status.status.success());
            doc_count += 1;
        }
    }
    let ingest_duration = start_ingest.elapsed();
    let avg_push = ingest_duration / doc_count as u32;
    let push_throughput = (doc_count as f64) / ingest_duration.as_secs_f64();
    println!("   -> Total ingested: {} notes", doc_count);
    println!("   -> Total ingestion time: {:?}", ingest_duration);
    println!(
        "   -> Average latency per note (push + incremental index + commit): {:?}",
        avg_push
    );
    println!("   -> Throughput: {:.1} notes/second", push_throughput);

    // 4. Batch Sync / Reindex Benchmark
    let start_sync = Instant::now();
    let sync_out = Command::new(exe)
        .arg("--vault")
        .arg(vault_path)
        .arg("sync")
        .arg("--force")
        .output()
        .unwrap();
    assert!(sync_out.status.success());
    let sync_duration = start_sync.elapsed();
    println!(
        "\n4. Full Sync / Reindex (60 docs from scratch): {:?}",
        sync_duration
    );

    // 5. Concurrency Stress Test: 8 simultaneous subagents pushing & logging
    println!("\n5. Concurrency Stress Test (8 parallel subagents writing simultaneously)...");
    let thread_count = 8;
    let ops_per_thread = 5;
    let success_counter = Arc::new(AtomicUsize::new(0));
    let mut handles = Vec::new();

    let start_concurrent = Instant::now();
    for t_id in 0..thread_count {
        let exe_clone = exe.to_string();
        let vault_clone = vault_path.to_string();
        let counter = Arc::clone(&success_counter);

        let handle = thread::spawn(move || {
            for i in 0..ops_per_thread {
                let note_name = format!("worker-thread-{}-note-{}", t_id, i);
                let content = format!(
                    "Subagent worker {} reporting status at step {}. Highly concurrent operations.",
                    t_id, i
                );

                let push = Command::new(&exe_clone)
                    .arg("--vault")
                    .arg(&vault_clone)
                    .arg("push")
                    .arg(&content)
                    .arg("--title")
                    .arg(&note_name)
                    .arg("--tags")
                    .arg("worker,parallel")
                    .output()
                    .unwrap();

                if push.status.success() {
                    counter.fetch_add(1, Ordering::SeqCst);
                }

                let log = Command::new(&exe_clone)
                    .arg("--vault")
                    .arg(&vault_clone)
                    .arg("log")
                    .arg("shared-session-audit")
                    .arg(format!("Worker {} completed step {}", t_id, i))
                    .output()
                    .unwrap();

                if log.status.success() {
                    counter.fetch_add(1, Ordering::SeqCst);
                }
            }
        });
        handles.push(handle);
    }

    for h in handles {
        h.join().unwrap();
    }
    let concurrent_duration = start_concurrent.elapsed();
    let total_concurrent_ops = thread_count * ops_per_thread * 2;
    let successful_ops = success_counter.load(Ordering::SeqCst);
    println!(
        "   -> Total concurrent operations: {}",
        total_concurrent_ops
    );
    println!(
        "   -> Successful operations without crash: {}/{}",
        successful_ops, total_concurrent_ops
    );
    println!(
        "   -> Total time for concurrent stress test: {:?}",
        concurrent_duration
    );
    assert_eq!(
        successful_ops, total_concurrent_ops,
        "All concurrent operations must succeed"
    );

    // 6. Search Latency & Token Budget Accuracy Test
    println!("\n6. BM25 Search & Token Truncation Benchmark...");
    let test_queries = [
        ("encryption", 50),
        ("encryption", 100),
        ("encryption", 200),
        ("postgres", 75),
        ("borrow checker", 120),
        ("concurrency", 150),
        ("token budget", 80),
        ("tantivy", 250),
    ];

    let bpe = tiktoken_rs::cl100k_base().unwrap();
    let mut total_search_duration = std::time::Duration::ZERO;

    for (query, token_budget) in test_queries {
        let start_q = Instant::now();
        let search_out = Command::new(exe)
            .arg("--vault")
            .arg(vault_path)
            .arg("get")
            .arg(query)
            .arg("--tokens")
            .arg(token_budget.to_string())
            .output()
            .unwrap();
        let q_time = start_q.elapsed();
        total_search_duration += q_time;

        assert!(
            search_out.status.success(),
            "Search for '{}' must succeed",
            query
        );
        let text = String::from_utf8_lossy(&search_out.stdout);
        let actual_tokens = bpe.encode_with_special_tokens(&text).len();

        println!(
            "   Query: {:<16} | Budget: {:>3} | Actual tokens: {:>3} | Latency: {:?}",
            query, token_budget, actual_tokens, q_time
        );
        assert!(
            actual_tokens <= token_budget,
            "Actual tokens ({}) exceeded budget ({}) for query '{}'",
            actual_tokens,
            token_budget,
            query
        );
    }
    println!(
        "   -> Average search latency: {:?}",
        total_search_duration / test_queries.len() as u32
    );

    // 7. Multi-hit RAG Context Packing Benchmark
    println!("\n7. Multi-hit RAG Context Packing (eng get --top 3 --tokens 400)...");
    let start_rag = Instant::now();
    let rag_out = Command::new(exe)
        .arg("--vault")
        .arg(vault_path)
        .arg("get")
        .arg("security OR encryption")
        .arg("--top")
        .arg("3")
        .arg("--tokens")
        .arg("400")
        .output()
        .unwrap();
    let rag_time = start_rag.elapsed();
    assert!(rag_out.status.success());
    let rag_text = String::from_utf8_lossy(&rag_out.stdout);
    let rag_tokens = bpe.encode_with_special_tokens(&rag_text).len();
    println!("   -> Packed 3 hits into single markdown context");
    println!("   -> Token budget: 400 | Actual tokens: {}", rag_tokens);
    println!("   -> Latency: {:?}", rag_time);
    assert!(rag_tokens <= 400);

    // 8. Grep & Links Performance
    println!("\n8. Grep & Graph Traversal Performance...");
    let start_grep = Instant::now();
    let grep_out = Command::new(exe)
        .arg("--vault")
        .arg(vault_path)
        .arg("grep")
        .arg("AES-256")
        .output()
        .unwrap();
    let grep_time = start_grep.elapsed();
    assert!(grep_out.status.success());
    println!("   -> Grep regex scan: {:?}", grep_time);

    let start_links = Instant::now();
    let links_out = Command::new(exe)
        .arg("--vault")
        .arg(vault_path)
        .arg("links")
        .arg("auth-jwt-v1")
        .output()
        .unwrap();
    let links_time = start_links.elapsed();
    assert!(links_out.status.success());
    println!("   -> Wikilink graph analysis: {:?}", links_time);

    // 9. Status and Vault Health
    println!("\n9. Vault Final Status...");
    let status_out = Command::new(exe)
        .arg("--vault")
        .arg(vault_path)
        .arg("status")
        .output()
        .unwrap();
    assert!(status_out.status.success());
    println!("{}", String::from_utf8_lossy(&status_out.stdout).trim());

    println!("========================================================");
    println!("      BATTLE TEST & BENCHMARK COMPLETED SUCCESSFULLY");
    println!("========================================================\n");
}
