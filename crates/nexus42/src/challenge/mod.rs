//! Challenge Solver Module
//!
//! Parses obfuscated math challenges from the platform's anti-bot verification
//! and computes the answer for `POST /creators/verify`.
//!
//! # Processing Pipeline
//!
//! 1. **Noise removal** — strip 8 noise symbols (`] ^ * | - ~ / [`)
//! 2. **Case normalization** — convert to lowercase
//! 3. **Number word conversion** — map English words to digits
//! 4. **Math extraction** — extract `{n1, op, n2}` from cleaned text
//! 5. **Evaluation** — compute result, guard division by zero, ensure non-negative
//!
//! # Errors
//!
//! Returns `ChallengeError` for invalid input, unrecognized patterns,
//! division by zero, or non-integer results.

#![allow(dead_code)]

pub mod eval;
pub mod noise;
pub mod numbers;
pub mod parser;

use eval::evaluate;
use thiserror::Error;

/// Errors that can occur during challenge solving.
#[derive(Debug, Error, PartialEq)]
pub enum ChallengeError {
    /// Challenge text is empty or contains only whitespace.
    #[error("challenge text is empty")]
    InvalidInput,

    /// Could not extract a math problem from the text.
    #[error("could not parse math problem from challenge text")]
    ParseError,

    /// Division by zero attempted.
    #[error("division by zero")]
    DivisionByZero,

    /// Subtraction produced a negative result.
    #[error("negative result: {n1} - {n2}")]
    NegativeResult { n1: u32, n2: u32 },

    /// Division produced a non-integer result.
    #[error("non-integer division result: {dividend} / {divisor}")]
    NonIntegerResult { dividend: u32, divisor: u32 },
}

/// Result type alias for challenge solving.
pub type Result<T> = std::result::Result<T, ChallengeError>;

/// Solve a challenge text and return the numeric answer as a string.
///
/// This is the main entry point. It runs the full processing pipeline:
/// noise removal → case normalization → number word conversion →
/// math extraction → evaluation.
///
/// # Arguments
///
/// * `text` - The raw challenge text from the platform.
///
/// # Returns
///
/// The computed answer as a string (e.g. `"47"`), or a `ChallengeError`.
///
/// # Example
///
/// ```
/// # use nexus42::challenge::solve_challenge;
/// let answer = solve_challenge(
///     "A bAs]KeT ^hAs tHiR*tY fI|vE ApPl-Es aNd ^sOmEoNe A*dDs ^TwEl/Ve Mo[Re, hOw MaN~y Ap-PlEs tO|tAl"
/// ).unwrap();
/// assert_eq!(answer, "47");
/// ```
pub fn solve_challenge(text: &str) -> Result<String> {
    let trimmed = text.trim();

    if trimmed.is_empty() {
        return Err(ChallengeError::InvalidInput);
    }

    // Step 1: Remove noise symbols
    let cleaned = noise::strip_noise(trimmed);

    // Step 2: Normalize to lowercase
    let lower = cleaned.to_lowercase();

    // Step 3: Convert English number words to digits
    let with_numbers = numbers::convert_number_words(&lower);

    // Step 4: Extract math problem
    let problem = parser::extract_math_problem(&with_numbers).ok_or(ChallengeError::ParseError)?;

    // Step 5: Evaluate
    let result = evaluate(&problem)?;

    Ok(result.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_example_returns_47() {
        let answer = solve_challenge(
            "A bAs]KeT ^hAs tHiR*tY fI|vE ApPl-Es aNd ^sOmEoNe A*dDs ^TwEl/Ve Mo[Re, hOw MaN~y Ap-PlEs tO|tAl",
        );
        assert_eq!(answer, Ok("47".to_string()));
    }

    #[test]
    fn empty_input_returns_error() {
        let result = solve_challenge("");
        assert!(matches!(result, Err(ChallengeError::InvalidInput)));
    }

    #[test]
    fn whitespace_only_returns_error() {
        let result = solve_challenge("   \t\n  ");
        assert!(matches!(result, Err(ChallengeError::InvalidInput)));
    }

    #[test]
    fn unrecognized_pattern_returns_error() {
        let result = solve_challenge("hello world");
        assert!(matches!(result, Err(ChallengeError::ParseError)));
    }

    #[test]
    fn subtraction_works() {
        let answer = solve_challenge(
            "A basket has fifty apples and someone subtracts twelve more, how many apples total",
        );
        assert_eq!(answer, Ok("38".to_string()));
    }

    #[test]
    fn multiplication_works() {
        let answer = solve_challenge(
            "A shelf has six books and someone multiplies seven more, how many books total",
        );
        assert_eq!(answer, Ok("42".to_string()));
    }

    #[test]
    fn division_works() {
        let answer = solve_challenge(
            "A classroom has fifty students and someone divides five more, how many students total",
        );
        assert_eq!(answer, Ok("10".to_string()));
    }

    #[test]
    fn division_by_zero_error() {
        let answer = solve_challenge(
            "A basket has ten apples and someone divides zero more, how many apples total",
        );
        assert!(matches!(answer, Err(ChallengeError::DivisionByZero)));
    }

    #[test]
    fn negative_result_error() {
        let answer = solve_challenge(
            "A basket has five apples and someone subtracts ten more, how many apples total",
        );
        assert!(matches!(answer, Err(ChallengeError::NegativeResult { .. })));
    }

    #[test]
    fn non_integer_division_error() {
        let answer = solve_challenge(
            "A basket has seven apples and someone divides two more, how many apples total",
        );
        assert!(matches!(
            answer,
            Err(ChallengeError::NonIntegerResult { .. })
        ));
    }

    #[test]
    fn extra_noise_symbols_handled() {
        let answer = solve_challenge(
            "A] bAsKeT ^hAs *tHiRtY fIvE aPpLeS |aNd sOmEoNe -aDdS tWeLvE mOrE, hOw ~mAnY aPpLeS /tOtAl",
        );
        assert_eq!(answer, Ok("47".to_string()));
    }

    #[test]
    fn simple_addition_no_noise() {
        let answer = solve_challenge(
            "A basket has five apples and someone adds three more, how many apples total",
        );
        assert_eq!(answer, Ok("8".to_string()));
    }

    #[test]
    fn compound_number_ninety_nine_plus_one() {
        let answer = solve_challenge(
            "A basket has ninety nine apples and someone adds one more, how many apples total",
        );
        assert_eq!(answer, Ok("100".to_string()));
    }
}
