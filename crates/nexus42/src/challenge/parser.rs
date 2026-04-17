//! Math problem extraction
//!
//! Extracts two numbers and one operation from the cleaned challenge text.
//! The text format is: "{scene} has {N1} {item} and someone {op_keyword} {N2} more, how many {item} total"

use super::eval::{MathProblem, Operation};

/// Operation keyword mapping.
struct OpMapping {
    keywords: &'static [&'static str],
    op: Operation,
}

/// All supported operation mappings, ordered from most specific to least.
const OP_MAPPINGS: &[OpMapping] = &[
    OpMapping {
        keywords: &["multiplies", "times"],
        op: Operation::Multiply,
    },
    OpMapping {
        keywords: &["divides", "splits", "divided by"],
        op: Operation::Divide,
    },
    OpMapping {
        keywords: &["subtracts", "removes", "takes", "minus"],
        op: Operation::Subtract,
    },
    OpMapping {
        keywords: &["adds", "and", "more", "plus"],
        op: Operation::Add,
    },
];

/// Extract a math problem from cleaned, normalized, number-converted text.
///
/// Returns `None` if the pattern is not recognized.
///
/// Expected pattern: "... has <N1> <item> and someone <op> <N2> more, how many ..."
/// where N1 and N2 are now digit strings after number conversion.
pub fn extract_math_problem(text: &str) -> Option<MathProblem> {
    // Extract the two numbers from the text (they are digit strings after conversion)
    let numbers: Vec<u32> = text
        .split_whitespace()
        .filter_map(|w| w.parse::<u32>().ok())
        .collect();

    if numbers.len() < 2 {
        return None;
    }

    let n1 = numbers[0];
    let n2 = numbers[1];

    // Detect the operation from keywords
    let lower = text.to_lowercase();
    let op = detect_operation(&lower)?;

    Some(MathProblem { n1, op, n2 })
}

/// Detect the arithmetic operation from keywords in the text.
fn detect_operation(text: &str) -> Option<Operation> {
    for mapping in OP_MAPPINGS {
        for keyword in mapping.keywords {
            if text.contains(keyword) {
                return Some(mapping.op);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_addition() {
        let text = "a basket has 35 apples and someone adds 12 more, how many apples total";
        let problem = extract_math_problem(text).expect("should extract");
        assert_eq!(problem.n1, 35);
        assert_eq!(problem.op, Operation::Add);
        assert_eq!(problem.n2, 12);
    }

    #[test]
    fn extracts_addition_with_and_keyword() {
        let text = "a basket has 35 apples and someone and 12 more, how many apples total";
        let problem = extract_math_problem(text).expect("should extract");
        assert_eq!(problem.op, Operation::Add);
    }

    #[test]
    fn extracts_subtraction() {
        let text = "a basket has 35 apples and someone subtracts 12 more, how many apples total";
        let problem = extract_math_problem(text).expect("should extract");
        assert_eq!(problem.n1, 35);
        assert_eq!(problem.op, Operation::Subtract);
        assert_eq!(problem.n2, 12);
    }

    #[test]
    fn extracts_multiplication() {
        let text = "a shelf has 6 books and someone multiplies 7 more, how many books total";
        let problem = extract_math_problem(text).expect("should extract");
        assert_eq!(problem.n1, 6);
        assert_eq!(problem.op, Operation::Multiply);
        assert_eq!(problem.n2, 7);
    }

    #[test]
    fn extracts_division() {
        let text =
            "a classroom has 100 students and someone divides 5 more, how many students total";
        let problem = extract_math_problem(text).expect("should extract");
        assert_eq!(problem.n1, 100);
        assert_eq!(problem.op, Operation::Divide);
        assert_eq!(problem.n2, 5);
    }

    #[test]
    fn returns_none_for_no_numbers() {
        let text = "hello world";
        assert!(extract_math_problem(text).is_none());
    }

    #[test]
    fn returns_none_for_single_number() {
        let text = "there are 42 things";
        assert!(extract_math_problem(text).is_none());
    }

    #[test]
    fn returns_none_for_no_operation_keyword() {
        // Use text without any operation keywords (note: "and" is a keyword, avoid it)
        let text = "a basket has 35 apples with 12 items appearing";
        assert!(extract_math_problem(text).is_none());
    }

    #[test]
    fn extracts_from_spec_example_pipeline() {
        // After noise removal, lowercasing, and number word conversion
        let text = "a basket has 35 apples and someone adds 12 more, how many apples total";
        let problem = extract_math_problem(text).expect("should extract");
        assert_eq!(problem.n1, 35);
        assert_eq!(problem.op, Operation::Add);
        assert_eq!(problem.n2, 12);
    }
}
