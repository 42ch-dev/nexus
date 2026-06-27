//! Arithmetic evaluation
//!
//! Computes the result of a parsed math problem.
//! Guards against division by zero and ensures non-negative integer result.

/// The four supported arithmetic operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operation {
    Add,
    Subtract,
    Multiply,
    Divide,
}

/// A parsed math problem: two operands and one operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MathProblem {
    pub n1: u32,
    pub op: Operation,
    pub n2: u32,
}

/// Evaluate a math problem, returning the result as a non-negative integer.
///
/// # Errors
///
/// Returns `ChallengeError` if:
/// - Division by zero is attempted
/// - The result is negative (for subtraction)
/// - The result is not a clean integer (for division)
pub const fn evaluate(problem: &MathProblem) -> Result<u32, crate::challenge::ChallengeError> {
    match problem.op {
        Operation::Add => Ok(problem.n1.saturating_add(problem.n2)),
        Operation::Subtract => {
            if problem.n2 > problem.n1 {
                return Err(crate::challenge::ChallengeError::NegativeResult {
                    n1: problem.n1,
                    n2: problem.n2,
                });
            }
            Ok(problem.n1 - problem.n2)
        }
        Operation::Multiply => Ok(problem.n1.saturating_mul(problem.n2)),
        Operation::Divide => {
            if problem.n2 == 0 {
                return Err(crate::challenge::ChallengeError::DivisionByZero);
            }
            if !problem.n1.is_multiple_of(problem.n2) {
                return Err(crate::challenge::ChallengeError::NonIntegerResult {
                    dividend: problem.n1,
                    divisor: problem.n2,
                });
            }
            Ok(problem.n1 / problem.n2)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addition() {
        let p = MathProblem {
            n1: 35,
            op: Operation::Add,
            n2: 12,
        };
        assert_eq!(evaluate(&p), Ok(47));
    }

    #[test]
    fn subtraction_positive() {
        let p = MathProblem {
            n1: 50,
            op: Operation::Subtract,
            n2: 12,
        };
        assert_eq!(evaluate(&p), Ok(38));
    }

    #[test]
    fn subtraction_zero() {
        let p = MathProblem {
            n1: 10,
            op: Operation::Subtract,
            n2: 10,
        };
        assert_eq!(evaluate(&p), Ok(0));
    }

    #[test]
    fn subtraction_negative_error() {
        let p = MathProblem {
            n1: 5,
            op: Operation::Subtract,
            n2: 10,
        };
        assert!(evaluate(&p).is_err());
    }

    #[test]
    fn multiplication() {
        let p = MathProblem {
            n1: 6,
            op: Operation::Multiply,
            n2: 7,
        };
        assert_eq!(evaluate(&p), Ok(42));
    }

    #[test]
    fn multiplication_zero() {
        let p = MathProblem {
            n1: 0,
            op: Operation::Multiply,
            n2: 100,
        };
        assert_eq!(evaluate(&p), Ok(0));
    }

    #[test]
    fn division_integer() {
        let p = MathProblem {
            n1: 100,
            op: Operation::Divide,
            n2: 5,
        };
        assert_eq!(evaluate(&p), Ok(20));
    }

    #[test]
    fn division_by_zero_error() {
        let p = MathProblem {
            n1: 10,
            op: Operation::Divide,
            n2: 0,
        };
        assert!(matches!(
            evaluate(&p),
            Err(crate::challenge::ChallengeError::DivisionByZero)
        ));
    }

    #[test]
    fn division_non_integer_error() {
        let p = MathProblem {
            n1: 7,
            op: Operation::Divide,
            n2: 2,
        };
        assert!(matches!(
            evaluate(&p),
            Err(crate::challenge::ChallengeError::NonIntegerResult { .. })
        ));
    }

    #[test]
    fn spec_example_thirty_five_plus_twelve() {
        let p = MathProblem {
            n1: 35,
            op: Operation::Add,
            n2: 12,
        };
        assert_eq!(evaluate(&p), Ok(47));
    }
}
