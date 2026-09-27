use std::process::Command;
use tempfile::tempdir;

#[test]
fn test_cli_lifecycle() {
    let dir = tempdir().unwrap();
    let vault_path = dir.path().to_str().unwrap();

    let exe = env!("CARGO_BIN_EXE_eng");

    // 1. init
    let output = Command::new(exe)
        .arg("init")
        .arg(vault_path)
        .output()
        .expect("Failed to run init");
    assert!(output.status.success());
    let init_stdout = String::from_utf8_lossy(&output.stdout);
    assert!(init_stdout.contains("Initialized eng vault at"));

    // 2. push note 1
    let output = Command::new(exe)
        .arg("--vault")
        .arg(vault_path)
        .arg("push")
        .arg("# Rust Guide\n\nRust is a memory-safe language without garbage collection.")
        .arg("--title")
        .arg("rust-guide")
        .arg("--tags")
        .arg("rust,safety,systems")
        .output()
        .expect("Failed to run push");
    assert!(output.status.success());
    let push_stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(push_stdout.trim(), "OK: saved rust-guide.md");

    // 3. push note 2 with wikilink
    let output = Command::new(exe)
        .arg("--vault")
        .arg(vault_path)
        .arg("push")
        .arg("Refer to [[rust-guide]] for memory safety rules.")
        .arg("--title")
        .arg("coding-standards")
        .output()
        .expect("Failed to run push 2");
    assert!(output.status.success());

    // 4. status
    let output = Command::new(exe)
        .arg("--vault")
        .arg(vault_path)
        .arg("status")
        .output()
        .expect("Failed to run status");
    assert!(output.status.success());
    let status_stdout = String::from_utf8_lossy(&output.stdout);
    assert!(status_stdout.contains("Notes: 2"));
    assert!(status_stdout.contains("Indexed: 2"));
    assert!(status_stdout.contains("Index status: ready"));

    // 5. get matching query
    let output = Command::new(exe)
        .arg("--vault")
        .arg(vault_path)
        .arg("get")
        .arg("memory-safe")
        .output()
        .expect("Failed to run get");
    assert!(output.status.success());
    let get_stdout = String::from_utf8_lossy(&output.stdout);
    assert!(get_stdout.contains("Rust is a memory-safe language"));

    // 6. get --format json
    let output = Command::new(exe)
        .arg("--vault")
        .arg(vault_path)
        .arg("get")
        .arg("memory-safe")
        .arg("--format")
        .arg("json")
        .output()
        .expect("Failed to run get json");
    assert!(output.status.success());
    let get_json_stdout = String::from_utf8_lossy(&output.stdout);
    assert!(get_json_stdout.contains("\"file\":\"rust-guide.md\""));
    assert!(get_json_stdout.contains("\"tokens\":"));

    // 7. get --list
    let output = Command::new(exe)
        .arg("--vault")
        .arg(vault_path)
        .arg("get")
        .arg("memory-safe")
        .arg("--list")
        .output()
        .expect("Failed to run get list");
    assert!(output.status.success());
    let list_stdout = String::from_utf8_lossy(&output.stdout);
    assert!(list_stdout.contains("1. rust-guide.md"));

    // 8. links
    let output = Command::new(exe)
        .arg("--vault")
        .arg(vault_path)
        .arg("links")
        .arg("rust-guide")
        .output()
        .expect("Failed to run links");
    assert!(output.status.success());
    let links_stdout = String::from_utf8_lossy(&output.stdout);
    assert!(links_stdout.contains("Backlinks:"));
    assert!(links_stdout.contains("coding-standards.md"));

    // 9. append
    let output = Command::new(exe)
        .arg("--vault")
        .arg(vault_path)
        .arg("push")
        .arg("## Concurrency\n\nRust concurrency is fearless.")
        .arg("--title")
        .arg("rust-guide")
        .arg("--append")
        .output()
        .expect("Failed to run push append");
    assert!(output.status.success());

    // 10. get --section
    let output = Command::new(exe)
        .arg("--vault")
        .arg(vault_path)
        .arg("get")
        .arg("Rust")
        .arg("--section")
        .arg("Concurrency")
        .output()
        .expect("Failed to run get section");
    assert!(output.status.success());
    let section_stdout = String::from_utf8_lossy(&output.stdout);
    assert!(section_stdout.contains("## Concurrency"));
    assert!(section_stdout.contains("Rust concurrency is fearless."));

    // 11. non-matching query returns exit code 1
    let output = Command::new(exe)
        .arg("--vault")
        .arg(vault_path)
        .arg("get")
        .arg("NonExistentTermXYZ")
        .output()
        .expect("Failed to run get non-existent");
    assert_eq!(output.status.code(), Some(1));

    // 12. read note directly
    let output = Command::new(exe)
        .arg("--vault")
        .arg(vault_path)
        .arg("read")
        .arg("rust-guide")
        .output()
        .expect("Failed to run read");
    assert!(output.status.success());
    let read_stdout = String::from_utf8_lossy(&output.stdout);
    assert!(read_stdout.contains("Rust is a memory-safe language"));

    // 13. read with --section and --format json
    let output = Command::new(exe)
        .arg("--vault")
        .arg(vault_path)
        .arg("read")
        .arg("rust-guide")
        .arg("--section")
        .arg("Concurrency")
        .arg("--format")
        .arg("json")
        .output()
        .expect("Failed to run read section json");
    assert!(output.status.success());
    let read_json = String::from_utf8_lossy(&output.stdout);
    assert!(read_json.contains("\"file\":\"rust-guide.md\""));
    assert!(read_json.contains("Rust concurrency is fearless."));

    // 14. ls
    let output = Command::new(exe)
        .arg("--vault")
        .arg(vault_path)
        .arg("ls")
        .output()
        .expect("Failed to run ls");
    assert!(output.status.success());
    let ls_stdout = String::from_utf8_lossy(&output.stdout);
    assert!(ls_stdout.contains("rust-guide.md [rust, safety, systems]"));
    assert!(ls_stdout.contains("coding-standards.md"));

    // 15. ls with --tag filter
    let output = Command::new(exe)
        .arg("--vault")
        .arg(vault_path)
        .arg("ls")
        .arg("--tag")
        .arg("rust")
        .output()
        .expect("Failed to run ls tag");
    assert!(output.status.success());
    let ls_tag_stdout = String::from_utf8_lossy(&output.stdout);
    assert!(ls_tag_stdout.contains("rust-guide.md"));
    assert!(!ls_tag_stdout.contains("coding-standards.md"));

    // 16. tags
    let output = Command::new(exe)
        .arg("--vault")
        .arg(vault_path)
        .arg("tags")
        .output()
        .expect("Failed to run tags");
    assert!(output.status.success());
    let tags_stdout = String::from_utf8_lossy(&output.stdout);
    assert!(tags_stdout.contains("rust (1)"));
    assert!(tags_stdout.contains("safety (1)"));

    // 17. count
    let output = Command::new(exe)
        .arg("--vault")
        .arg(vault_path)
        .arg("count")
        .arg("This is a simple sentence with tokens.")
        .output()
        .expect("Failed to run count string");
    assert!(output.status.success());
    let count_num: usize = String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse()
        .expect("Expected numeric token count");
    assert!(count_num > 0);

    // count note file directly
    let output = Command::new(exe)
        .arg("--vault")
        .arg(vault_path)
        .arg("count")
        .arg("rust-guide")
        .output()
        .expect("Failed to run count note");
    assert!(output.status.success());
    let note_tokens: usize = String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse()
        .expect("Expected numeric token count");
    assert!(note_tokens > 10);

    // 18. sync
    let output = Command::new(exe)
        .arg("--vault")
        .arg(vault_path)
        .arg("sync")
        .output()
        .expect("Failed to run sync");
    assert!(output.status.success());
    let sync_stdout = String::from_utf8_lossy(&output.stdout);
    assert!(sync_stdout.contains("OK: indexed 2 notes"));

    // 19. rm note
    let output = Command::new(exe)
        .arg("--vault")
        .arg(vault_path)
        .arg("rm")
        .arg("coding-standards")
        .output()
        .expect("Failed to run rm");
    assert!(output.status.success());
    let rm_stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(rm_stdout.trim(), "OK: deleted coding-standards.md");

    // verify deleted note cannot be read
    let output = Command::new(exe)
        .arg("--vault")
        .arg(vault_path)
        .arg("read")
        .arg("coding-standards")
        .output()
        .expect("Failed to run read on deleted note");
    assert_eq!(output.status.code(), Some(1));

    // verify status shows 1 note remaining
    let output = Command::new(exe)
        .arg("--vault")
        .arg(vault_path)
        .arg("status")
        .output()
        .expect("Failed to run status");
    assert!(output.status.success());
    let status_stdout = String::from_utf8_lossy(&output.stdout);
    assert!(status_stdout.contains("Notes: 1"));
    assert!(status_stdout.contains("Indexed: 1"));

    // 20. no args prints help
    let output = Command::new(exe)
        .output()
        .expect("Failed to run eng without arguments");
    let help_out = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(help_out.contains("Usage:") || help_out.contains("Commands:"));

    // 21. which and path
    let output = Command::new(exe)
        .arg("--vault")
        .arg(vault_path)
        .arg("path")
        .output()
        .expect("Failed to run path");
    assert!(output.status.success());
    let path_stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!path_stdout.trim().is_empty());

    let output = Command::new(exe)
        .arg("--vault")
        .arg(vault_path)
        .arg("which")
        .arg("rust-guide")
        .output()
        .expect("Failed to run which");
    assert!(output.status.success());
    let which_stdout = String::from_utf8_lossy(&output.stdout);
    assert!(which_stdout.contains("rust-guide.md"));

    // 22. log
    let output = Command::new(exe)
        .arg("--vault")
        .arg(vault_path)
        .arg("log")
        .arg("audit")
        .arg("Initial security review completed")
        .output()
        .expect("Failed to run log");
    assert!(output.status.success());
    let log_stdout = String::from_utf8_lossy(&output.stdout);
    assert!(log_stdout.contains("OK: logged to audit.md"));

    let output = Command::new(exe)
        .arg("--vault")
        .arg(vault_path)
        .arg("read")
        .arg("audit")
        .output()
        .expect("Failed to read audit");
    assert!(output.status.success());
    let audit_read = String::from_utf8_lossy(&output.stdout);
    assert!(audit_read.contains("Initial security review completed"));
    assert!(audit_read.contains("UTC]"));

    // 23. grep
    let output = Command::new(exe)
        .arg("--vault")
        .arg(vault_path)
        .arg("grep")
        .arg("security review")
        .output()
        .expect("Failed to run grep");
    assert!(output.status.success());
    let grep_stdout = String::from_utf8_lossy(&output.stdout);
    assert!(grep_stdout.contains("audit.md:"));
    assert!(grep_stdout.contains("security review"));

    // 24. tag add, list, rm
    let output = Command::new(exe)
        .arg("--vault")
        .arg(vault_path)
        .arg("tag")
        .arg("add")
        .arg("audit")
        .arg("security")
        .arg("p1")
        .output()
        .expect("Failed to run tag add");
    assert!(output.status.success());

    let output = Command::new(exe)
        .arg("--vault")
        .arg(vault_path)
        .arg("tag")
        .arg("list")
        .arg("audit")
        .output()
        .expect("Failed to run tag list");
    assert!(output.status.success());
    let tag_list = String::from_utf8_lossy(&output.stdout);
    assert!(tag_list.contains("p1"));
    assert!(tag_list.contains("security"));

    let output = Command::new(exe)
        .arg("--vault")
        .arg(vault_path)
        .arg("tag")
        .arg("rm")
        .arg("audit")
        .arg("p1")
        .output()
        .expect("Failed to run tag rm");
    assert!(output.status.success());

    let output = Command::new(exe)
        .arg("--vault")
        .arg(vault_path)
        .arg("tag")
        .arg("list")
        .arg("audit")
        .output()
        .expect("Failed to run tag list after rm");
    let tag_list2 = String::from_utf8_lossy(&output.stdout);
    assert!(!tag_list2.contains("p1"));
    assert!(tag_list2.contains("security"));

    // 25. orphans
    let output = Command::new(exe)
        .arg("--vault")
        .arg(vault_path)
        .arg("orphans")
        .output()
        .expect("Failed to run orphans");
    assert!(output.status.success());
    let orphans_stdout = String::from_utf8_lossy(&output.stdout);
    assert!(orphans_stdout.contains("audit.md"));

    // 26. push via stdin
    use std::io::Write;
    let mut child = Command::new(exe)
        .arg("--vault")
        .arg(vault_path)
        .arg("push")
        .arg("-")
        .arg("--title")
        .arg("scratchpad")
        .arg("--tags")
        .arg("temp")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("Failed to spawn push via stdin");

    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(b"Temporary notes from pipeline")
        .unwrap();
    let child_out = child.wait_with_output().unwrap();
    assert!(child_out.status.success());
    assert!(String::from_utf8_lossy(&child_out.stdout).contains("OK: saved scratchpad.md"));

    // 27. multi-hit RAG packing (eng get --top 2)
    let output = Command::new(exe)
        .arg("--vault")
        .arg(vault_path)
        .arg("get")
        .arg("notes OR security")
        .arg("--top")
        .arg("2")
        .output()
        .expect("Failed to run get --top 2");
    assert!(output.status.success());
    let multi_stdout = String::from_utf8_lossy(&output.stdout);
    assert!(multi_stdout.contains("### "));

    // 28. prune --tag temp
    let output = Command::new(exe)
        .arg("--vault")
        .arg(vault_path)
        .arg("prune")
        .arg("--tag")
        .arg("temp")
        .output()
        .expect("Failed to run prune");
    assert!(output.status.success());
    let prune_stdout = String::from_utf8_lossy(&output.stdout);
    assert!(prune_stdout.contains("OK: pruned 1 notes with tag 'temp'"));

    // verify scratchpad is gone
    let output = Command::new(exe)
        .arg("--vault")
        .arg(vault_path)
        .arg("read")
        .arg("scratchpad")
        .output()
        .expect("Failed to run read on pruned note");
    assert_eq!(output.status.code(), Some(1));

    // 29. edit --section
    let output = Command::new(exe)
        .arg("--vault")
        .arg(vault_path)
        .arg("edit")
        .arg("rust-guide")
        .arg("--section")
        .arg("Advanced Ownership")
        .arg("Deep dive into interior mutability and RefCell.")
        .output()
        .expect("Failed to edit section");
    assert!(output.status.success());
    let edit_sec_out = String::from_utf8_lossy(&output.stdout);
    assert!(edit_sec_out.contains("OK: updated section 'Advanced Ownership'"));

    let output = Command::new(exe)
        .arg("--vault")
        .arg(vault_path)
        .arg("read")
        .arg("rust-guide")
        .arg("--section")
        .arg("Advanced Ownership")
        .output()
        .expect("Failed to read updated section");
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("interior mutability"));

    // 30. edit --find and --replace
    let output = Command::new(exe)
        .arg("--vault")
        .arg(vault_path)
        .arg("edit")
        .arg("rust-guide")
        .arg("--find")
        .arg("interior mutability")
        .arg("--replace")
        .arg("cell primitives")
        .output()
        .expect("Failed to run edit --find");
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("OK: replaced 1 occurrences"));

    // 31. edit piped stdin preserving frontmatter tags
    let mut child = Command::new(exe)
        .arg("--vault")
        .arg(vault_path)
        .arg("edit")
        .arg("audit")
        .arg("-")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("Failed to spawn edit via stdin");

    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(b"Brand new audit report body after full scan.")
        .unwrap();
    let child_out = child.wait_with_output().unwrap();
    assert!(child_out.status.success());
    assert!(String::from_utf8_lossy(&child_out.stdout).contains("OK: updated body of audit"));

    let output = Command::new(exe)
        .arg("--vault")
        .arg(vault_path)
        .arg("read")
        .arg("audit")
        .output()
        .expect("Failed to read audit after body update");
    let audit_read_after = String::from_utf8_lossy(&output.stdout);
    assert!(audit_read_after.contains("Brand new audit report body"));

    // Verify tags are still intact
    let output = Command::new(exe)
        .arg("--vault")
        .arg(vault_path)
        .arg("tag")
        .arg("list")
        .arg("audit")
        .output()
        .expect("Failed to list tags");
    let audit_tags = String::from_utf8_lossy(&output.stdout);
    assert!(audit_tags.contains("security"));
    assert!(!audit_tags.contains("p1"));

    // 32. Verify Tantivy index reflects edited changes immediately
    let output = Command::new(exe)
        .arg("--vault")
        .arg(vault_path)
        .arg("get")
        .arg("cell primitives")
        .output()
        .expect("Failed to get search for edited term");
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("cell primitives"));
}
