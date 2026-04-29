//! English number word conversion
//!
//! Maps English number words (0-100) to digit strings.
//! Supports compound numbers like "thirty five" → "35".

use std::collections::HashMap;

/// Build the static lookup table for number words.
fn build_number_map() -> HashMap<&'static str, u32> {
    let mut m = HashMap::new();
    // Units
    m.insert("zero", 0);
    m.insert("one", 1);
    m.insert("two", 2);
    m.insert("three", 3);
    m.insert("four", 4);
    m.insert("five", 5);
    m.insert("six", 6);
    m.insert("seven", 7);
    m.insert("eight", 8);
    m.insert("nine", 9);
    // Teens
    m.insert("ten", 10);
    m.insert("eleven", 11);
    m.insert("twelve", 12);
    m.insert("thirteen", 13);
    m.insert("fourteen", 14);
    m.insert("fifteen", 15);
    m.insert("sixteen", 16);
    m.insert("seventeen", 17);
    m.insert("eighteen", 18);
    m.insert("nineteen", 19);
    // Tens
    m.insert("twenty", 20);
    m.insert("thirty", 30);
    m.insert("forty", 40);
    m.insert("fifty", 50);
    m.insert("sixty", 60);
    m.insert("seventy", 70);
    m.insert("eighty", 80);
    m.insert("ninety", 90);
    // Hundred
    m.insert("hundred", 100);
    m
}

/// Get the static number word map.
fn number_map() -> &'static HashMap<&'static str, u32> {
    use std::sync::LazyLock;
    static MAP: LazyLock<HashMap<&'static str, u32>> = LazyLock::new(build_number_map);
    &MAP
}

/// Convert all English number words in the text to digit strings.
///
/// Handles:
/// - Simple words: "twelve" → "12"
/// - Compound tens + units: "thirty five" → "35"
///
/// Text is assumed to be lowercase and noise-free.
#[must_use]
pub fn convert_number_words(text: &str) -> String {
    let map = number_map();
    let words: Vec<&str> = text.split_whitespace().collect();
    let mut result = Vec::with_capacity(words.len());
    let mut i = 0;

    while i < words.len() {
        if let Some(&val) = map.get(words[i]) {
            // Check if next word is a units digit that could form a compound number
            if (20..=90).contains(&val) && i + 1 < words.len() {
                if let Some(&next_val) = map.get(words[i + 1]) {
                    if (1..=9).contains(&next_val) {
                        // Compound: "thirty five" → 35
                        result.push((val + next_val).to_string());
                        i += 2;
                        continue;
                    }
                }
            }
            result.push(val.to_string());
        } else {
            result.push(words[i].to_string());
        }
        i += 1;
    }

    result.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_zero_to_nineteen() {
        assert_eq!(convert_number_words("zero"), "0");
        assert_eq!(convert_number_words("one"), "1");
        assert_eq!(convert_number_words("two"), "2");
        assert_eq!(convert_number_words("three"), "3");
        assert_eq!(convert_number_words("four"), "4");
        assert_eq!(convert_number_words("five"), "5");
        assert_eq!(convert_number_words("six"), "6");
        assert_eq!(convert_number_words("seven"), "7");
        assert_eq!(convert_number_words("eight"), "8");
        assert_eq!(convert_number_words("nine"), "9");
        assert_eq!(convert_number_words("ten"), "10");
        assert_eq!(convert_number_words("eleven"), "11");
        assert_eq!(convert_number_words("twelve"), "12");
        assert_eq!(convert_number_words("thirteen"), "13");
        assert_eq!(convert_number_words("fourteen"), "14");
        assert_eq!(convert_number_words("fifteen"), "15");
        assert_eq!(convert_number_words("sixteen"), "16");
        assert_eq!(convert_number_words("seventeen"), "17");
        assert_eq!(convert_number_words("eighteen"), "18");
        assert_eq!(convert_number_words("nineteen"), "19");
    }

    #[test]
    fn converts_tens() {
        assert_eq!(convert_number_words("twenty"), "20");
        assert_eq!(convert_number_words("thirty"), "30");
        assert_eq!(convert_number_words("forty"), "40");
        assert_eq!(convert_number_words("fifty"), "50");
        assert_eq!(convert_number_words("sixty"), "60");
        assert_eq!(convert_number_words("seventy"), "70");
        assert_eq!(convert_number_words("eighty"), "80");
        assert_eq!(convert_number_words("ninety"), "90");
    }

    #[test]
    fn converts_hundred() {
        assert_eq!(convert_number_words("hundred"), "100");
    }

    #[test]
    fn converts_compound_numbers() {
        assert_eq!(convert_number_words("twenty one"), "21");
        assert_eq!(convert_number_words("thirty five"), "35");
        assert_eq!(convert_number_words("forty two"), "42");
        assert_eq!(convert_number_words("fifty nine"), "59");
        assert_eq!(convert_number_words("sixty three"), "63");
        assert_eq!(convert_number_words("seventy eight"), "78");
        assert_eq!(convert_number_words("eighty nine"), "89");
        assert_eq!(convert_number_words("ninety nine"), "99");
    }

    #[test]
    fn converts_in_sentence_context() {
        assert_eq!(
            convert_number_words("a basket has thirty five apples and someone adds twelve more"),
            "a basket has 35 apples and someone adds 12 more"
        );
    }

    #[test]
    fn preserves_non_number_words() {
        assert_eq!(convert_number_words("hello world"), "hello world");
    }

    #[test]
    fn handles_empty_input() {
        assert_eq!(convert_number_words(""), "");
    }

    #[test]
    fn handles_multiple_numbers_in_text() {
        assert_eq!(
            convert_number_words("there are twelve items and five more"),
            "there are 12 items and 5 more"
        );
    }

    #[test]
    fn does_not_mangle_unknown_words() {
        assert_eq!(convert_number_words("some random text"), "some random text");
    }
}
