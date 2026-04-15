//! Content quality signal scoring for review classification.
//!
//! Pure functions that analyze text to determine whether digest content
//! has high informational value or is repetitive noise. Used by
//! `classify_pending_review()` to gate promotion decisions.

use std::collections::HashMap;

/// Signal metrics extracted from text content.
#[derive(Debug, Clone)]
pub struct QualitySignal {
    /// Ratio of unique tokens to total tokens (0.0 – 1.0).
    pub unique_ratio: f32,
    /// Ratio of alphabetic characters to total non-whitespace characters (0.0 – 1.0).
    pub alpha_ratio: f32,
    /// Ratio of the most frequent token's count to total tokens (0.0 – 1.0).
    /// High values indicate repetition.
    pub repeated_token_ratio: f32,
}

/// Tokenize text on whitespace and compute quality signal metrics.
///
/// This is a pure function with no I/O or external dependencies.
/// Tokens are lowercased for deduplication but alpha ratio is computed
/// against the original text.
pub fn quality_signal(input: &str) -> QualitySignal {
    let tokens: Vec<&str> = input.split_whitespace().collect();
    let total = tokens.len();

    if total == 0 {
        return QualitySignal {
            unique_ratio: 0.0,
            alpha_ratio: 0.0,
            repeated_token_ratio: 0.0,
        };
    }

    // Unique ratio: unique lowercased tokens / total tokens
    let mut seen: HashMap<&str, usize> = HashMap::new();
    let mut max_count: usize = 0;
    for tok in &tokens {
        let lower = tok.to_lowercase();
        let entry = seen.entry(Box::leak(lower.into_boxed_str())).or_insert(0);
        *entry += 1;
        if *entry > max_count {
            max_count = *entry;
        }
    }
    let unique_count = seen.len();
    let unique_ratio = unique_count as f32 / total as f32;
    let repeated_token_ratio = max_count as f32 / total as f32;

    // Alpha ratio: alphabetic chars / non-whitespace chars in original text
    let alpha_count = input.chars().filter(|c| c.is_alphabetic()).count();
    let non_ws_count = input.chars().filter(|c| !c.is_whitespace()).count();
    let alpha_ratio = if non_ws_count > 0 {
        alpha_count as f32 / non_ws_count as f32
    } else {
        0.0
    };

    QualitySignal {
        unique_ratio,
        alpha_ratio,
        repeated_token_ratio,
    }
}

/// Determine whether text has high informational signal.
///
/// Thresholds tuned to reject repetitive noise while accepting
/// substantive creative content:
/// - At least 35% unique tokens (rejects >65% repetition)
/// - At least 65% alphabetic characters (rejects symbol-heavy noise)
/// - No single token exceeds 45% of all tokens (rejects single-word spam)
pub fn is_high_signal(input: &str) -> bool {
    let q = quality_signal(input);
    q.unique_ratio >= 0.35 && q.alpha_ratio >= 0.65 && q.repeated_token_ratio <= 0.45
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_string_yields_zero_signals() {
        let q = quality_signal("");
        assert_eq!(q.unique_ratio, 0.0);
        assert_eq!(q.alpha_ratio, 0.0);
        assert_eq!(q.repeated_token_ratio, 0.0);
        assert!(!is_high_signal(""));
    }

    #[test]
    fn repetitive_noise_is_low_signal() {
        let noise = "aaa aaa aaa aaa aaa aaa aaa aaa aaa aaa ".repeat(40);
        assert!(!is_high_signal(&noise));
        let q = quality_signal(&noise);
        assert!(
            q.unique_ratio < 0.35,
            "unique_ratio should be low: {}",
            q.unique_ratio
        );
        assert!(
            q.repeated_token_ratio > 0.45,
            "repeated_token_ratio should be high: {}",
            q.repeated_token_ratio
        );
    }

    #[test]
    fn rich_text_is_high_signal() {
        let text = "The chapter pivots from betrayal to alliance, with causal consequences for three factions.";
        assert!(is_high_signal(text));
        let q = quality_signal(text);
        assert!(q.unique_ratio >= 0.35, "unique_ratio: {}", q.unique_ratio);
        assert!(q.alpha_ratio >= 0.65, "alpha_ratio: {}", q.alpha_ratio);
        assert!(
            q.repeated_token_ratio <= 0.45,
            "repeated_token_ratio: {}",
            q.repeated_token_ratio
        );
    }

    #[test]
    fn mostly_symbols_is_low_signal() {
        let symbols = "!@# $%^ &*() !@# $%^ &*() !@# $%^ &*() !@# $%^";
        assert!(!is_high_signal(symbols));
        let q = quality_signal(symbols);
        assert!(
            q.alpha_ratio < 0.65,
            "alpha_ratio should be low: {}",
            q.alpha_ratio
        );
    }

    #[test]
    fn mixed_content_with_stop_words_still_high_signal() {
        let text = "Discussed three key themes for the novel: narrative structure, character arcs, and emotional resonance.";
        assert!(is_high_signal(text));
    }
}
