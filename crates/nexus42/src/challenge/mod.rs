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
//! # LLM Fallback
//!
//! When the pure logic pipeline fails with `ChallengeError::ParseError`, the solver
//! can fall back to an LLM invocation via the [`LlmSolver`] trait. The LLM receives
//! the challenge text as user input and `challenge-solver-skill.md`
//! (embedded at compile time via [`CHALLENGE_SOLVER_SYSTEM_PROMPT`])
//! as the system prompt. If the LLM is unavailable or its response fails numeric
//! validation, the original parse error is returned.

#![allow(dead_code)]

pub mod eval;
pub mod noise;
pub mod numbers;
pub mod parser;

use eval::evaluate;
use thiserror::Error;

/// System prompt content from the challenge-solver skill file, embedded at compile time.
pub const CHALLENGE_SOLVER_SYSTEM_PROMPT: &str =
    include_str!("../skills/challenge-solver-skill.md");

/// Errors that can occur during challenge solving.
#[derive(Debug, Error, PartialEq, Eq)]
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

/// Trait for LLM-based challenge solving (fallback path).
///
/// Implementations send the challenge text to an LLM with the
/// challenge-solver skill file as system prompt and return the
/// LLM's numeric answer.
///
/// If the LLM is unavailable or returns an error, implementations
/// should return `None` so the caller falls through to the original
/// parse error.
pub trait LlmSolver: Send + Sync {
    /// Attempt to solve a challenge using LLM.
    ///
    /// * `challenge_text` - the raw challenge text from the platform.
    ///
    /// Returns `Some(numeric_answer)` on success, or `None` if the LLM
    /// is unavailable, times out, or produces an invalid response.
    async fn solve(&self, challenge_text: &str) -> Option<String>;
}

/// Default [`LlmSolver`] that always returns `None`, indicating unavailability.
///
/// Used as the default solver in production when no real LLM provider is
/// configured. This makes the fallback infrastructure active without
/// requiring an LLM provider.
pub struct UnavailableLlmSolver;

impl LlmSolver for UnavailableLlmSolver {
    async fn solve(&self, _challenge_text: &str) -> Option<String> {
        None
    }
}

/// Default timeout (in seconds) for LLM fallback calls.
const LLM_FALLBACK_TIMEOUT_SECS: u64 = 30;

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
/// # Errors
///
/// Returns `ChallengeError::InvalidInput` if the text is empty or whitespace.
/// Returns `ChallengeError::ParseError` if no math problem can be extracted.
/// Returns `ChallengeError::DivisionByZero` if division by zero is attempted.
/// Returns `ChallengeError::NegativeResult` if subtraction produces a negative.
/// Returns `ChallengeError::NonIntegerResult` if division produces a non-integer.
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

/// Solve a challenge with LLM fallback when the pure logic pipeline fails.
///
/// First attempts the pure logic pipeline. If it returns `ChallengeError::ParseError`,
/// invokes the provided [`LlmSolver`] as a fallback. The LLM response is validated
/// to ensure it contains only a numeric value before acceptance.
///
/// If the LLM solver is unavailable (returns `None`), the original parse error is
/// returned with a warning logged.
///
/// # Arguments
///
/// * `text` - The raw challenge text from the platform.
/// * `llm` - An implementation of [`LlmSolver`] for LLM fallback.
///
/// # Returns
///
/// The computed answer as a string, or a `ChallengeError`.
///
/// # Errors
///
/// Returns the same errors as `solve_challenge`, except `ParseError` may be
/// resolved by the LLM fallback. If LLM fallback fails or times out,
/// `ParseError` is returned.
pub async fn solve_challenge_with_fallback<L>(text: &str, llm: &L) -> Result<String>
where
    L: LlmSolver,
{
    match solve_challenge(text) {
        ok @ Ok(_) => ok,
        Err(ChallengeError::ParseError) => {
            // TODO(telemetry): emit `challenge.fallback.attempted` counter when
            // metrics infrastructure is available. Track outcome (success/failure/
            // timeout/unavailable) as labelled counters.
            tracing::warn!("pure logic pipeline failed, attempting LLM fallback for challenge");
            let llm_result = tokio::time::timeout(
                std::time::Duration::from_secs(LLM_FALLBACK_TIMEOUT_SECS),
                llm.solve(text),
            )
            .await;

            match llm_result {
                Ok(Some(response)) => {
                    let trimmed = response.trim().to_string();
                    if is_valid_numeric_answer(&trimmed) {
                        tracing::info!("LLM fallback succeeded with answer: {}", trimmed);
                        Ok(trimmed)
                    } else {
                        tracing::warn!("LLM fallback returned invalid numeric format: {}", trimmed);
                        Err(ChallengeError::ParseError)
                    }
                }
                Ok(None) => {
                    tracing::warn!("LLM fallback unavailable, returning original parse error");
                    Err(ChallengeError::ParseError)
                }
                Err(_) => {
                    tracing::warn!(
                        "LLM fallback timed out after {}s, returning original parse error",
                        LLM_FALLBACK_TIMEOUT_SECS
                    );
                    Err(ChallengeError::ParseError)
                }
            }
        }
        err => err,
    }
}

/// Check if a string is a valid numeric answer (optional leading minus, digits).
fn is_valid_numeric_answer(s: &str) -> bool {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return false;
    }
    // Allow optional leading minus for negative numbers, followed by digits
    trimmed
        .chars()
        .all(|c| c.is_ascii_digit() || (c == '-' && trimmed.starts_with('-') && trimmed.len() > 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockLlmSolver {
        response: Option<String>,
    }

    impl MockLlmSolver {
        fn new(response: Option<&str>) -> Self {
            Self {
                response: response.map(String::from),
            }
        }
    }

    impl LlmSolver for MockLlmSolver {
        async fn solve(&self, _challenge_text: &str) -> Option<String> {
            self.response.clone()
        }
    }

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

    // --- LLM fallback tests ---

    #[tokio::test]
    async fn fallback_solves_when_pure_logic_fails() {
        let llm = MockLlmSolver::new(Some("42"));
        let result = solve_challenge_with_fallback("hello world", &llm).await;
        assert_eq!(result, Ok("42".to_string()));
    }

    #[tokio::test]
    async fn fallback_returns_parse_error_when_llm_unavailable() {
        let llm = MockLlmSolver::new(None);
        let result = solve_challenge_with_fallback("hello world", &llm).await;
        assert!(matches!(result, Err(ChallengeError::ParseError)));
    }

    #[tokio::test]
    async fn fallback_rejects_non_numeric_llm_response() {
        let llm = MockLlmSolver::new(Some("the answer is forty-two"));
        let result = solve_challenge_with_fallback("hello world", &llm).await;
        assert!(matches!(result, Err(ChallengeError::ParseError)));
    }

    #[tokio::test]
    async fn fallback_trims_whitespace_from_llm_response() {
        let llm = MockLlmSolver::new(Some("  123  "));
        let result = solve_challenge_with_fallback("hello world", &llm).await;
        assert_eq!(result, Ok("123".to_string()));
    }

    #[tokio::test]
    async fn fallback_skipped_when_pure_logic_succeeds() {
        let llm = MockLlmSolver::new(None); // LLM unavailable
        let result = solve_challenge_with_fallback(
            "A basket has five apples and someone adds three more, how many apples total",
            &llm,
        )
        .await;
        assert_eq!(result, Ok("8".to_string()));
    }

    #[tokio::test]
    async fn fallback_skipped_for_non_parse_errors() {
        let llm = MockLlmSolver::new(Some("999"));
        // Division by zero should NOT trigger LLM fallback
        let result = solve_challenge_with_fallback(
            "A basket has ten apples and someone divides zero more, how many apples total",
            &llm,
        )
        .await;
        assert!(matches!(result, Err(ChallengeError::DivisionByZero)));
    }

    struct SlowLlmSolver {
        delay: std::time::Duration,
    }

    impl LlmSolver for SlowLlmSolver {
        async fn solve(&self, _challenge_text: &str) -> Option<String> {
            tokio::time::sleep(self.delay).await;
            Some("42".to_string())
        }
    }

    #[tokio::test]
    async fn fallback_returns_parse_error_on_timeout() {
        let llm = SlowLlmSolver {
            delay: std::time::Duration::from_mins(1), // longer than 30s timeout
        };
        let result = solve_challenge_with_fallback("hello world", &llm).await;
        assert!(
            matches!(result, Err(ChallengeError::ParseError)),
            "timeout should fall through to original parse error"
        );
    }

    // --- is_valid_numeric_answer tests ---

    #[test]
    fn valid_numeric_plain() {
        assert!(is_valid_numeric_answer("42"));
    }

    #[test]
    fn valid_numeric_zero() {
        assert!(is_valid_numeric_answer("0"));
    }

    #[test]
    fn valid_numeric_large() {
        assert!(is_valid_numeric_answer("999999"));
    }

    #[test]
    fn valid_numeric_negative() {
        assert!(is_valid_numeric_answer("-5"));
    }

    #[test]
    fn invalid_numeric_empty() {
        assert!(!is_valid_numeric_answer(""));
    }

    #[test]
    fn invalid_numeric_letters() {
        assert!(!is_valid_numeric_answer("forty-two"));
    }

    #[test]
    fn invalid_numeric_mixed() {
        assert!(!is_valid_numeric_answer("42 apples"));
    }

    #[test]
    fn invalid_numeric_minus_only() {
        assert!(!is_valid_numeric_answer("-"));
    }

    #[test]
    fn invalid_numeric_whitespace() {
        assert!(!is_valid_numeric_answer("  "));
    }
}
