use std::sync::OnceLock;
use tiktoken_rs::{cl100k_base, CoreBPE};

static BPE: OnceLock<CoreBPE> = OnceLock::new();

pub fn get_bpe() -> &'static CoreBPE {
    BPE.get_or_init(|| cl100k_base().expect("Failed to initialize cl100k_base tokenizer"))
}

pub fn count_tokens(text: &str) -> usize {
    let bpe = get_bpe();
    bpe.encode_with_special_tokens(text).len()
}

/// Truncate `text` so that its token count does not exceed `max_tokens`.
/// Tries to break on sentence boundaries (e.g. `. `, `\n\n`, `\n`) when possible.
/// If even the first sentence exceeds `max_tokens`, cuts strictly at `max_tokens`.
pub fn truncate_to_tokens(text: &str, max_tokens: usize) -> (String, usize) {
    if max_tokens == 0 {
        return (String::new(), 0);
    }

    let bpe = get_bpe();
    let all_tokens = bpe.encode_with_special_tokens(text);
    if all_tokens.len() <= max_tokens {
        return (text.to_string(), all_tokens.len());
    }

    // Split text into candidate sentences/segments
    let sentences = split_sentences(text);
    let mut accumulated = String::new();
    let mut last_valid = String::new();
    let mut last_valid_tokens = 0;

    for s in sentences {
        let candidate = if accumulated.is_empty() {
            s.to_string()
        } else {
            format!("{}{}", accumulated, s)
        };

        let candidate_tokens = bpe.encode_with_special_tokens(&candidate);
        if candidate_tokens.len() <= max_tokens {
            last_valid = candidate.clone();
            last_valid_tokens = candidate_tokens.len();
            accumulated = candidate;
        } else {
            break;
        }
    }

    if !last_valid.is_empty() {
        return (last_valid.trim_end().to_string(), last_valid_tokens);
    }

    // If even the first sentence exceeds max_tokens, decode strictly max_tokens
    let truncated_tokens = &all_tokens[..max_tokens];
    let decoded = bpe
        .decode(truncated_tokens.to_vec())
        .unwrap_or_else(|_| text.chars().take(max_tokens * 3).collect());
    let actual_count = bpe.encode_with_special_tokens(&decoded).len();
    (decoded.trim_end().to_string(), actual_count)
}

/// Split text into sentences or segments while preserving whitespace/delimiters.
fn split_sentences(text: &str) -> Vec<&str> {
    let mut segments = Vec::new();
    let mut start = 0;
    let bytes = text.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        if bytes[i] == b'\n' {
            if i + 1 < len && bytes[i + 1] == b'\n' {
                // Double newline paragraph break
                segments.push(&text[start..=i + 1]);
                i += 2;
                start = i;
                continue;
            } else {
                segments.push(&text[start..=i]);
                i += 1;
                start = i;
                continue;
            }
        }

        if (bytes[i] == b'.' || bytes[i] == b'!' || bytes[i] == b'?')
            && (i + 1 == len || bytes[i + 1].is_ascii_whitespace())
        {
            // Advance past trailing whitespace
            let mut end = i + 1;
            while end < len && bytes[end].is_ascii_whitespace() && bytes[end] != b'\n' {
                end += 1;
            }
            segments.push(&text[start..end]);
            start = end;
            i = end;
            continue;
        }

        i += 1;
    }

    if start < len {
        segments.push(&text[start..len]);
    }

    segments
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_count_tokens() {
        let count = count_tokens("Hello world");
        assert_eq!(count, 2);
    }

    #[test]
    fn test_truncate_sentences() {
        let text = "First sentence here. Second sentence follows. Third sentence ends it.";
        let (res, tokens) = truncate_to_tokens(text, 8);
        assert!(tokens <= 8);
        assert!(res.contains("First sentence here."));
    }

    #[test]
    fn test_truncate_long_single_sentence() {
        let text = "ThisIsAVeryLongSentenceWithoutAnyPunctuationThatWillExceedBudgetByFar";
        let (res, tokens) = truncate_to_tokens(text, 3);
        assert!(tokens <= 3);
        assert!(!res.is_empty());
    }
}
