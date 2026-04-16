//! Noise removal layer
//!
//! Strips the 8 noise symbols used by the platform to obfuscate challenge text:
//! `]`, `^`, `*`, `|`, `-`, `~`, `/`, `[`

/// Noise symbols to remove from challenge text.
const NOISE_SYMBOLS: &[char] = &[']', '^', '*', '|', '-', '~', '/', '['];

/// Strip all noise symbols from the given text.
///
/// Returns a new string with all 8 noise characters removed.
/// This is the first step in the challenge-solving pipeline.
pub fn strip_noise(text: &str) -> String {
    text.chars()
        .filter(|c| !NOISE_SYMBOLS.contains(c))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_all_eight_noise_symbols() {
        let input = "a]b^c*d|e-f~g/h[i";
        assert_eq!(strip_noise(input), "abcdefghi");
    }

    #[test]
    fn strips_noise_from_spec_example() {
        let input = "A bAs]KeT ^hAs tHiR*tY fI|vE ApPl-Es aNd ^sOmEoNe A*dDs ^TwEl/Ve Mo[Re";
        let result = strip_noise(input);
        assert_eq!(
            result,
            "A bAsKeT hAs tHiRtY fIvE ApPlEs aNd sOmEoNe AdDs TwElVe MoRe"
        );
        // None of the noise symbols should remain
        for sym in NOISE_SYMBOLS {
            assert!(
                !result.contains(*sym),
                "noise symbol '{}' still present",
                sym
            );
        }
    }

    #[test]
    fn handles_empty_input() {
        assert_eq!(strip_noise(""), "");
    }

    #[test]
    fn preserves_text_without_noise() {
        assert_eq!(strip_noise("hello world"), "hello world");
    }

    #[test]
    fn strips_only_noise_symbols_leaving_punctuation() {
        // Comma and period should be preserved
        assert_eq!(strip_noise("hello, world."), "hello, world.");
    }

    #[test]
    fn strips_consecutive_noise_symbols() {
        assert_eq!(strip_noise("a]]]b^^^c"), "abc");
    }

    #[test]
    fn strips_noise_at_boundaries() {
        assert_eq!(strip_noise("]hello["), "hello");
    }

    #[test]
    fn all_noise_symbols_individually() {
        for sym in NOISE_SYMBOLS {
            let input = format!("a{}b", sym);
            assert_eq!(strip_noise(&input), "ab", "failed to strip '{}'", sym);
        }
    }
}
