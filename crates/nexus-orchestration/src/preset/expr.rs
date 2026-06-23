//! Simple expression grammar for conditional routing (V1.56 P2, DF-56 sub-item 2).
//!
//! Parses simple boolean/matching expressions against `graph_flow::Context` JSON values:
//! - **Field access**: `_context.<dotted.path>` resolves to a JSON value from context
//! - **Comparisons**: `==`, `!=`, `>`, `<`, `>=`, `<=`
//! - **Boolean**: `&&`, `||`, `!`, parens
//! - **Literals**: numbers (integers + floats), strings (single/double quoted), booleans, `null`
//!
//! ## Grammar (PEG-ish)
//!
//! ```text
//! expr     = or_expr
//! or_expr  = and_expr ("||" and_expr)*
//! and_expr = unary_expr ("&&" unary_expr)*
//! unary_expr = "!" unary_expr | primary
//! primary  = "(" expr ")" | comparison | literal | field_access
//! comparison = field_access cmp_op literal
//!            | literal cmp_op field_access
//!            | field_access cmp_op field_access
//! cmp_op   = "==" | "!=" | ">=" | "<=" | ">" | "<"
//! field_access = "_context" ("." IDENTIFIER)*
//! literal  = NUMBER | STRING | "true" | "false" | "null"
//! ```
//!
//! ## Evaluation
//!
//! * Expression evaluates against `graph_flow::Context` — `_context.x.y` resolves
//!   via context key lookup then JSON pointer traversal.
//! * Missing fields return `null`; `null == null` → `true`, `null != "x"` → `true`,
//!   `null > 0` → false (JSON semantics; see M-001 spec alignment).
//! * Type mismatches (comparing string to int) produce a typed error.
//!
//! ## Depth limit
//!
//! * Expression nesting depth is bounded by [`MAX_EXPR_DEPTH`] (= 32) to prevent
//!   stack overflow from user-installable presets with deeply-nested `when:` expressions.
//! * Depth counter is incremented at each recursive descent (parens, binary ops, unary NOT).
//! * Exceeding the limit returns [`ExprError::DepthExceeded`].

use std::fmt;

/// Maximum expression nesting depth to prevent stack overflow from deeply
/// nested `when:` expressions in user-installable presets (V1.56 P2 fix-wave, W-003).
pub const MAX_EXPR_DEPTH: u32 = 32;

/// Expression AST node.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    /// Boolean literal (`true` / `false`).
    Bool(bool),
    /// Numeric literal (stored as f64; integer literals are normalized).
    Number(f64),
    /// String literal (without surrounding quotes).
    Str(String),
    /// Null literal.
    Null,
    /// Context field access: `_context.<path>`.
    FieldAccess { path: Vec<String> },
    /// Comparison: `lhs op rhs`.
    Comparison {
        lhs: Box<Self>,
        op: CmpOp,
        rhs: Box<Self>,
    },
    /// Logical AND: `lhs && rhs`.
    And(Box<Self>, Box<Self>),
    /// Logical OR: `lhs || rhs`.
    Or(Box<Self>, Box<Self>),
    /// Logical NOT: `!inner`.
    Not(Box<Self>),
}

/// Comparison operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CmpOp {
    Eq,
    Ne,
    Gt,
    Lt,
    Ge,
    Le,
}

impl fmt::Display for CmpOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Eq => write!(f, "=="),
            Self::Ne => write!(f, "!="),
            Self::Gt => write!(f, ">"),
            Self::Lt => write!(f, "<"),
            Self::Ge => write!(f, ">="),
            Self::Le => write!(f, "<="),
        }
    }
}

/// Errors that can occur during expression parsing or evaluation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExprError {
    /// Unexpected character or token at a given position.
    Parse {
        pos: usize,
        message: String,
        full_expr: String,
    },
    /// Expected a value but reached end of input.
    UnexpectedEnd { message: String, full_expr: String },
    /// Field not found in context.
    FieldNotFound { path: String },
    /// Type mismatch (e.g. comparing string to int).
    TypeError { message: String },
    /// Expression exceeds maximum nesting depth (V1.56 P2 fix-wave, W-003).
    DepthExceeded(u32),
}

impl fmt::Display for ExprError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse {
                pos,
                message,
                full_expr,
            } => {
                write!(f, "parse error at position {pos} ({message}): {full_expr}")
            }
            Self::UnexpectedEnd { message, full_expr } => {
                write!(f, "unexpected end of expression ({message}): {full_expr}")
            }
            Self::FieldNotFound { path } => {
                write!(f, "field not found in context: {path}")
            }
            Self::TypeError { message } => {
                write!(f, "type error: {message}")
            }
            Self::DepthExceeded(depth) => {
                write!(
                    f,
                    "expression exceeds maximum nesting depth ({depth} > {MAX_EXPR_DEPTH})"
                )
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Parser
// ---------------------------------------------------------------------------

/// Parse an expression string into an AST.
///
/// # Errors
///
/// Returns [`ExprError::Parse`] if the input contains invalid syntax,
/// [`ExprError::UnexpectedEnd`] if the expression is truncated, or
/// [`ExprError::DepthExceeded`] if the expression exceeds [`MAX_EXPR_DEPTH`].
pub fn parse(input: &str) -> Result<Expr, ExprError> {
    let tokens = tokenize(input);
    let mut parser = Parser {
        tokens,
        pos: 0,
        input: input.to_string(),
        depth: 0,
    };
    let expr = parser.parse_or_expr()?;
    // Ensure we consumed all tokens.
    if parser.pos < parser.tokens.len() {
        let remaining: String = parser.tokens[parser.pos..]
            .iter()
            .map(Token::as_str)
            .collect::<Vec<_>>()
            .join(" ");
        return Err(ExprError::Parse {
            pos: parser.tokens[parser.pos].token_span().0,
            message: format!("unexpected trailing tokens: {remaining}"),
            full_expr: input.to_string(),
        });
    }
    Ok(expr)
}

#[derive(Debug, Clone)]
enum Token {
    Ident(String, (usize, usize)),
    StringLit(String, (usize, usize)),
    NumberLit(f64, (usize, usize)),
    True((usize, usize)),
    False((usize, usize)),
    Null((usize, usize)),
    EqEq((usize, usize)),
    Ne((usize, usize)),
    Ge((usize, usize)),
    Le((usize, usize)),
    Gt((usize, usize)),
    Lt((usize, usize)),
    AndAnd((usize, usize)),
    OrOr((usize, usize)),
    Bang((usize, usize)),
    LParen((usize, usize)),
    RParen((usize, usize)),
    Dot((usize, usize)),
}

impl Token {
    const fn token_span(&self) -> (usize, usize) {
        match self {
            Self::Ident(_, s)
            | Self::StringLit(_, s)
            | Self::NumberLit(_, s)
            | Self::True(s)
            | Self::False(s)
            | Self::Null(s)
            | Self::EqEq(s)
            | Self::Ne(s)
            | Self::Ge(s)
            | Self::Le(s)
            | Self::Gt(s)
            | Self::Lt(s)
            | Self::AndAnd(s)
            | Self::OrOr(s)
            | Self::Bang(s)
            | Self::LParen(s)
            | Self::RParen(s)
            | Self::Dot(s) => *s,
        }
    }

    fn as_str(&self) -> String {
        match self {
            Self::Ident(s, _) => s.clone(),
            Self::StringLit(s, _) => format!("\"{s}\""),
            Self::NumberLit(n, _) => n.to_string(),
            Self::True(_) => "true".to_string(),
            Self::False(_) => "false".to_string(),
            Self::Null(_) => "null".to_string(),
            Self::EqEq(_) => "==".to_string(),
            Self::Ne(_) => "!=".to_string(),
            Self::Ge(_) => ">=".to_string(),
            Self::Le(_) => "<=".to_string(),
            Self::Gt(_) => ">".to_string(),
            Self::Lt(_) => "<".to_string(),
            Self::AndAnd(_) => "&&".to_string(),
            Self::OrOr(_) => "||".to_string(),
            Self::Bang(_) => "!".to_string(),
            Self::LParen(_) => "(".to_string(),
            Self::RParen(_) => ")".to_string(),
            Self::Dot(_) => ".".to_string(),
        }
    }
}

#[allow(clippy::too_many_lines)]
fn tokenize(input: &str) -> Vec<Token> {
    let chars: Vec<char> = input.chars().collect();
    let len = chars.len();
    let mut tokens = Vec::new();
    let mut i = 0;

    while i < len {
        let ch = chars[i];

        // Whitespace
        if ch.is_whitespace() {
            i += 1;
            continue;
        }

        // Two-char operators
        if i + 1 < len {
            let next = chars[i + 1];
            match (ch, next) {
                ('=', '=') => {
                    tokens.push(Token::EqEq((i, i + 2)));
                    i += 2;
                    continue;
                }
                ('!', '=') => {
                    tokens.push(Token::Ne((i, i + 2)));
                    i += 2;
                    continue;
                }
                ('>', '=') => {
                    tokens.push(Token::Ge((i, i + 2)));
                    i += 2;
                    continue;
                }
                ('<', '=') => {
                    tokens.push(Token::Le((i, i + 2)));
                    i += 2;
                    continue;
                }
                ('&', '&') => {
                    tokens.push(Token::AndAnd((i, i + 2)));
                    i += 2;
                    continue;
                }
                ('|', '|') => {
                    tokens.push(Token::OrOr((i, i + 2)));
                    i += 2;
                    continue;
                }
                _ => {}
            }
        }

        // Single-char tokens
        match ch {
            '>' => {
                tokens.push(Token::Gt((i, i + 1)));
                i += 1;
                continue;
            }
            '<' => {
                tokens.push(Token::Lt((i, i + 1)));
                i += 1;
                continue;
            }
            '!' => {
                tokens.push(Token::Bang((i, i + 1)));
                i += 1;
                continue;
            }
            '(' => {
                tokens.push(Token::LParen((i, i + 1)));
                i += 1;
                continue;
            }
            ')' => {
                tokens.push(Token::RParen((i, i + 1)));
                i += 1;
                continue;
            }
            '.' => {
                tokens.push(Token::Dot((i, i + 1)));
                i += 1;
                continue;
            }
            _ => {}
        }

        // String literals (single or double quoted)
        if ch == '\'' || ch == '"' {
            let quote = ch;
            let start = i;
            i += 1; // skip opening quote
            let mut s = String::new();
            while i < len && chars[i] != quote {
                if chars[i] == '\\' && i + 1 < len {
                    i += 1;
                    match chars[i] {
                        'n' => s.push('\n'),
                        't' => s.push('\t'),
                        '\\' => s.push('\\'),
                        c if c == quote => s.push(quote),
                        c => {
                            s.push('\\');
                            s.push(c);
                        }
                    }
                } else {
                    s.push(chars[i]);
                }
                i += 1;
            }
            if i < len {
                i += 1; // skip closing quote
            }
            tokens.push(Token::StringLit(s, (start, i)));
            continue;
        }

        // Numbers
        if ch.is_ascii_digit() || (ch == '-' && i + 1 < len && chars[i + 1].is_ascii_digit()) {
            let start = i;
            let mut num_str = String::new();
            if ch == '-' {
                num_str.push('-');
                i += 1;
            }
            while i < len && (chars[i].is_ascii_digit() || chars[i] == '.') {
                num_str.push(chars[i]);
                i += 1;
            }
            let n: f64 = num_str.parse().unwrap_or(0.0);
            tokens.push(Token::NumberLit(n, (start, i)));
            continue;
        }

        // Identifiers / keywords
        if ch.is_ascii_alphabetic() || ch == '_' {
            let start = i;
            let mut ident = String::new();
            while i < len && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                ident.push(chars[i]);
                i += 1;
            }
            match ident.as_str() {
                "true" => tokens.push(Token::True((start, i))),
                "false" => tokens.push(Token::False((start, i))),
                "null" => tokens.push(Token::Null((start, i))),
                _ => tokens.push(Token::Ident(ident, (start, i))),
            }
            continue;
        }

        // Unknown character
        return vec![Token::Ident(
            format!("<unexpected char '{ch}'>"),
            (i, i + 1),
        )];
    }

    tokens
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    input: String,
    depth: u32,
}

impl Parser {
    /// Check and bump depth. Returns `Err(DepthExceeded)` if depth exceeds `MAX_EXPR_DEPTH`.
    const fn check_depth(&mut self) -> Result<(), ExprError> {
        self.depth += 1;
        if self.depth > MAX_EXPR_DEPTH {
            return Err(ExprError::DepthExceeded(self.depth));
        }
        Ok(())
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn advance(&mut self) -> Option<&Token> {
        let tok = self.tokens.get(self.pos);
        if tok.is_some() {
            self.pos += 1;
        }
        // Return the token we just advanced past (at old position).
        self.tokens.get(self.pos - 1)
    }

    fn pos(&self) -> usize {
        self.peek().map_or(self.input.len(), |t| t.token_span().0)
    }

    fn err(&self, message: &str) -> ExprError {
        ExprError::Parse {
            pos: self.pos(),
            message: message.to_string(),
            full_expr: self.input.clone(),
        }
    }

    fn is_oror(&self) -> bool {
        matches!(self.peek(), Some(Token::OrOr(_)))
    }

    fn is_andand(&self) -> bool {
        matches!(self.peek(), Some(Token::AndAnd(_)))
    }

    fn is_bang(&self) -> bool {
        matches!(self.peek(), Some(Token::Bang(_)))
    }

    // expr = or_expr
    fn parse_or_expr(&mut self) -> Result<Expr, ExprError> {
        let mut left = self.parse_and_expr()?;
        while self.is_oror() {
            self.advance();
            self.check_depth()?;
            let right = self.parse_and_expr()?;
            left = Expr::Or(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_and_expr(&mut self) -> Result<Expr, ExprError> {
        let mut left = self.parse_unary()?;
        while self.is_andand() {
            self.advance();
            self.check_depth()?;
            let right = self.parse_unary()?;
            left = Expr::And(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_unary(&mut self) -> Result<Expr, ExprError> {
        if self.is_bang() {
            self.advance();
            self.check_depth()?;
            let inner = self.parse_unary()?;
            return Ok(Expr::Not(Box::new(inner)));
        }
        self.parse_primary()
    }

    fn parse_primary(&mut self) -> Result<Expr, ExprError> {
        match self.peek() {
            Some(Token::LParen(_)) => {
                self.advance();
                self.check_depth()?;
                let expr = self.parse_or_expr()?;
                match self.peek() {
                    Some(Token::RParen(_)) => {
                        self.advance();
                        Ok(expr)
                    }
                    _ => Err(self.err("expected ')'")),
                }
            }
            Some(Token::Ident(_, _)) => {
                // Could be field_access or comparison if followed by cmp_op,
                // or a bare field_access (truthy check).
                let field = self.parse_field_access()?;
                // Peek for comparison operator
                let cmp_op = match self.peek() {
                    Some(Token::EqEq(_)) => Some(CmpOp::Eq),
                    Some(Token::Ne(_)) => Some(CmpOp::Ne),
                    Some(Token::Ge(_)) => Some(CmpOp::Ge),
                    Some(Token::Le(_)) => Some(CmpOp::Le),
                    Some(Token::Gt(_)) => Some(CmpOp::Gt),
                    Some(Token::Lt(_)) => Some(CmpOp::Lt),
                    _ => None,
                };
                if let Some(op) = cmp_op {
                    self.advance();
                    // RHS can be a literal OR another field access.
                    let rhs = self.parse_primary()?;
                    Ok(Expr::Comparison {
                        lhs: Box::new(field),
                        op,
                        rhs: Box::new(rhs),
                    })
                } else {
                    // Bare field access — truthy check at eval time.
                    Ok(field)
                }
            }
            Some(
                Token::StringLit(_, _)
                | Token::NumberLit(_, _)
                | Token::True(_)
                | Token::False(_)
                | Token::Null(_),
            ) => {
                let lhs = self.parse_value()?;
                // Check if it's followed by a comparison operator (for cases like `"abc" == field`)
                let cmp_op = match self.peek() {
                    Some(Token::EqEq(_)) => Some(CmpOp::Eq),
                    Some(Token::Ne(_)) => Some(CmpOp::Ne),
                    _ => None,
                };
                if let Some(op) = cmp_op {
                    self.advance();
                    let rhs = self.parse_primary()?;
                    Ok(Expr::Comparison {
                        lhs: Box::new(lhs),
                        op,
                        rhs: Box::new(rhs),
                    })
                } else {
                    Ok(lhs)
                }
            }
            _ => Err(self.err("expected expression")),
        }
    }

    /// Parse a contextual field access: `_context` [ `.` IDENT ]*
    fn parse_field_access(&mut self) -> Result<Expr, ExprError> {
        let tok = self.advance();
        let Some(tok) = tok else {
            return Err(self.err("expected identifier"));
        };
        let mut path = match tok {
            Token::Ident(name, _) => vec![name.clone()],
            _ => return Err(self.err("expected identifier for field access")),
        };

        // `_context` is the required root for context field access.
        if path[0] != "_context" {
            return Err(ExprError::Parse {
                pos: tok.token_span().0,
                message: format!("expected '_context' for field access, found '{}'", path[0]),
                full_expr: self.input.clone(),
            });
        }

        // Collect dot-separated path segments.
        while matches!(self.peek(), Some(Token::Dot(_))) {
            self.advance(); // skip dot
            match self.advance() {
                Some(Token::Ident(name, _)) => path.push(name.clone()),
                _ => return Err(self.err("expected identifier after '.'")),
            }
        }

        Ok(Expr::FieldAccess {
            path: path[1..].to_vec(), // drop the "_context" prefix
        })
    }

    /// Parse a literal value (not a field access).
    fn parse_value(&mut self) -> Result<Expr, ExprError> {
        let tok = self.advance();
        match tok {
            Some(Token::StringLit(s, _)) => Ok(Expr::Str(s.clone())),
            Some(Token::NumberLit(n, _)) => Ok(Expr::Number(*n)),
            Some(Token::True(_)) => Ok(Expr::Bool(true)),
            Some(Token::False(_)) => Ok(Expr::Bool(false)),
            Some(Token::Null(_)) => Ok(Expr::Null),
            _ => Err(self.err("expected literal value")),
        }
    }
}

// ---------------------------------------------------------------------------
// Evaluator
// ---------------------------------------------------------------------------

/// Evaluate a parsed expression against a JSON context object.
///
/// # Errors
///
/// Returns [`ExprError::FieldNotFound`] if a field path references a
/// non-existent key, or [`ExprError::TypeError`] on type mismatches.
pub fn evaluate(expr: &Expr, context: &serde_json::Value) -> Result<bool, ExprError> {
    // R-V156P2-L002: debug span capturing input context shape + expression
    // kind + result. Kept at `debug!` so it is no-op by default; enable with
    // `RUST_LOG=nexus_orchestration::preset::expr=debug` when diagnosing
    // conditional-routing misfires.
    let result = evaluate_inner(expr, context);
    tracing::debug!(
        expr_kind = ?expr.variant_name(),
        context_keys = %context_summary(context),
        result = ?result.as_ref().copied().ok(),
        error = ?result.as_ref().err(),
        "expression_eval: evaluated"
    );
    result
}

/// Compact one-line summary of a JSON context for debug logging (top-level
/// keys only — avoids dumping large nested values).
fn context_summary(context: &serde_json::Value) -> String {
    match context {
        serde_json::Value::Object(map) => {
            let keys: Vec<&str> = map.keys().map(String::as_str).collect();
            format!("{{{}}}", keys.join(","))
        }
        _ => format!("<{}>", context_type_name(context)),
    }
}

/// Short type name for a JSON value (debug aid).
const fn context_type_name(v: &serde_json::Value) -> &'static str {
    match v {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "bool",
        serde_json::Value::Number(_) => "number",
        serde_json::Value::String(_) => "string",
        serde_json::Value::Array(_) => "array",
        serde_json::Value::Object(_) => "object",
    }
}

impl Expr {
    /// Debug-friendly variant name (R-V156P2-L002 tracing aid).
    const fn variant_name(&self) -> &'static str {
        match self {
            Self::Bool(_) => "Bool",
            Self::Number(_) => "Number",
            Self::Str(_) => "Str",
            Self::Null => "Null",
            Self::FieldAccess { .. } => "FieldAccess",
            Self::Comparison { .. } => "Comparison",
            Self::And(_, _) => "And",
            Self::Or(_, _) => "Or",
            Self::Not(_) => "Not",
        }
    }
}

/// Inner recursive evaluation (entry point for the public `evaluate` wrapper).
fn evaluate_inner(expr: &Expr, context: &serde_json::Value) -> Result<bool, ExprError> {
    match expr {
        Expr::Bool(b) => Ok(*b),
        Expr::Number(n) => Ok(*n != 0.0),
        Expr::Str(s) => Ok(!s.is_empty()),
        Expr::Null => Ok(false),
        Expr::FieldAccess { path } => {
            let val = resolve_field(context, path);
            Ok(is_truthy(val))
        }
        Expr::Comparison { lhs, op, rhs } => {
            let lv = eval_value(lhs, context)?;
            let rv = eval_value(rhs, context)?;
            compare(&lv, *op, &rv)
        }
        Expr::And(left, right) => {
            let l = evaluate_inner(left, context)?;
            if !l {
                return Ok(false); // short-circuit
            }
            evaluate_inner(right, context)
        }
        Expr::Or(left, right) => {
            let l = evaluate_inner(left, context)?;
            if l {
                return Ok(true); // short-circuit
            }
            evaluate_inner(right, context)
        }
        Expr::Not(inner) => {
            let v = evaluate_inner(inner, context)?;
            Ok(!v)
        }
    }
}

/// Resolve a dotted path in a JSON value, returning `&Value::Null` for missing fields.
fn resolve_field<'a>(root: &'a serde_json::Value, path: &[String]) -> &'a serde_json::Value {
    static NULL: serde_json::Value = serde_json::Value::Null;
    let mut current = root;
    for seg in path {
        match current {
            serde_json::Value::Object(map) => {
                current = map.get(seg).unwrap_or(&NULL);
            }
            _ => return &NULL,
        }
    }
    current
}

/// Evaluate a sub-expression as a JSON value (for comparison operands).
fn eval_value(expr: &Expr, context: &serde_json::Value) -> Result<serde_json::Value, ExprError> {
    match expr {
        Expr::Bool(b) => Ok(serde_json::Value::Bool(*b)),
        Expr::Number(n) => Ok(serde_json::json!(*n)),
        Expr::Str(s) => Ok(serde_json::Value::String(s.clone())),
        Expr::Null => Ok(serde_json::Value::Null),
        Expr::FieldAccess { path } => Ok(resolve_field(context, path).clone()),
        Expr::Comparison { .. } | Expr::And(..) | Expr::Or(..) | Expr::Not(_) => {
            let b = evaluate_inner(expr, context)?;
            Ok(serde_json::Value::Bool(b))
        }
    }
}

/// Truthy check: non-null, non-false, non-zero, non-empty.
fn is_truthy(val: &serde_json::Value) -> bool {
    match val {
        serde_json::Value::Null => false,
        serde_json::Value::Bool(b) => *b,
        serde_json::Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0),
        serde_json::Value::String(s) => !s.is_empty(),
        serde_json::Value::Array(a) => !a.is_empty(),
        serde_json::Value::Object(o) => !o.is_empty(),
    }
}

/// Compare two JSON values with a comparison operator.
fn compare(lhs: &serde_json::Value, op: CmpOp, rhs: &serde_json::Value) -> Result<bool, ExprError> {
    match op {
        CmpOp::Eq => Ok(json_eq(lhs, rhs)),
        CmpOp::Ne => Ok(!json_eq(lhs, rhs)),
        CmpOp::Gt | CmpOp::Lt | CmpOp::Ge | CmpOp::Le => {
            let lf = to_f64(lhs).ok_or_else(|| ExprError::TypeError {
                message: format!("left operand is not a number: {lhs}"),
            })?;
            let rf = to_f64(rhs).ok_or_else(|| ExprError::TypeError {
                message: format!("right operand is not a number: {rhs}"),
            })?;
            let ok = match op {
                CmpOp::Gt => lf > rf,
                CmpOp::Lt => lf < rf,
                CmpOp::Ge => lf >= rf,
                CmpOp::Le => lf <= rf,
                _ => unreachable!(),
            };
            Ok(ok)
        }
    }
}

// ---------------------------------------------------------------------------
// Context dependency scanning (V1.56 P3 — DF-56 dependent slice)
// ---------------------------------------------------------------------------

/// What external context data a conditional expression depends on.
///
/// Used by the orchestration runtime to decide whether to invoke
/// `registry.refresh` or query workspace session state before evaluating
/// branches.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ContextDeps {
    /// Expression references `_context.registry_refresh.*`
    pub needs_registry_refresh: bool,
    /// Expression references `_context.workspace.*`
    pub needs_workspace: bool,
}

/// Scan an expression AST for context field dependencies.
///
/// Walks the tree recursively, collecting any `FieldAccess` paths whose first
/// segment is `registry_refresh` or `workspace`.
#[must_use]
pub fn scan_context_deps(expr: &Expr) -> ContextDeps {
    let mut deps = ContextDeps::default();
    scan_expr(expr, &mut deps);
    deps
}

/// Recursively scan an expression node, OR-ing dependencies into `deps`.
fn scan_expr(expr: &Expr, deps: &mut ContextDeps) {
    match expr {
        Expr::FieldAccess { path } => {
            if let Some(first) = path.first() {
                match first.as_str() {
                    "registry_refresh" => deps.needs_registry_refresh = true,
                    "workspace" => deps.needs_workspace = true,
                    _ => {}
                }
            }
        }
        Expr::Comparison { lhs, op: _, rhs } | Expr::And(lhs, rhs) | Expr::Or(lhs, rhs) => {
            scan_expr(lhs, deps);
            scan_expr(rhs, deps);
        }
        Expr::Not(inner) => scan_expr(inner, deps),
        Expr::Bool(_) | Expr::Number(_) | Expr::Str(_) | Expr::Null => {}
    }
}

fn json_eq(a: &serde_json::Value, b: &serde_json::Value) -> bool {
    match (a, b) {
        (serde_json::Value::Null, serde_json::Value::Null) => true,
        (serde_json::Value::Bool(a), serde_json::Value::Bool(b)) => a == b,
        (serde_json::Value::Number(a), serde_json::Value::Number(b)) => {
            // Compare numerically.
            a.as_f64() == b.as_f64()
        }
        (serde_json::Value::String(a), serde_json::Value::String(b)) => a == b,
        _ => false,
    }
}

fn to_f64(v: &serde_json::Value) -> Option<f64> {
    match v {
        serde_json::Value::Number(n) => n.as_f64(),
        serde_json::Value::String(s) => s.parse().ok(),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn evaluate_expr(expr_str: &str, ctx: &serde_json::Value) -> Result<bool, ExprError> {
        let ast = parse(expr_str)?;
        evaluate(&ast, ctx)
    }

    fn make_ctx(fields: &[(&str, serde_json::Value)]) -> serde_json::Value {
        let mut map = serde_json::Map::new();
        for (k, v) in fields {
            map.insert(k.to_string(), v.clone());
        }
        serde_json::Value::Object(map)
    }

    // ── Parser tests ────────────────────────────────────────────────

    #[test]
    fn parse_simple_equality_v2() {
        let expr = parse("_context.field == \"hello\"").unwrap();
        match expr {
            Expr::Comparison { op, .. } => assert_eq!(op, CmpOp::Eq),
            _ => panic!("expected Comparison"),
        }
    }

    #[test]
    fn parse_boolean_and() {
        let expr = parse("_context.a > 10 && _context.b < 5").unwrap();
        assert!(matches!(expr, Expr::And(..)));
    }

    #[test]
    fn parse_boolean_or() {
        let expr = parse("_context.a || _context.b").unwrap();
        assert!(matches!(expr, Expr::Or(..)));
    }

    #[test]
    fn parse_not() {
        let expr = parse("!_context.flag").unwrap();
        assert!(matches!(expr, Expr::Not(_)));
    }

    #[test]
    fn parse_parens() {
        let expr = parse("(_context.a > 1) && (_context.b < 2)").unwrap();
        assert!(matches!(expr, Expr::And(..)));
    }

    #[test]
    fn parse_nested_parens() {
        let expr = parse("((_context.a))").unwrap();
        // ((_context.a)) → parens around FieldAccess, which stays as bare FieldAccess
        // (truthy check at eval time).
        assert!(matches!(expr, Expr::FieldAccess { .. }));
    }

    #[test]
    fn parse_literals() {
        let expr = parse("true").unwrap();
        assert_eq!(expr, Expr::Bool(true));

        let expr = parse("false").unwrap();
        assert_eq!(expr, Expr::Bool(false));

        let expr = parse("null").unwrap();
        assert_eq!(expr, Expr::Null);

        let expr = parse("42").unwrap();
        assert_eq!(expr, Expr::Number(42.0));

        let expr = parse("-3.14").unwrap();
        assert_eq!(expr, Expr::Number(-3.14));

        let expr = parse("\"hello world\"").unwrap();
        assert_eq!(expr, Expr::Str("hello world".into()));
    }

    #[test]
    fn parse_single_quoted_string() {
        let expr = parse("'hello'").unwrap();
        assert_eq!(expr, Expr::Str("hello".into()));
    }

    #[test]
    fn parse_field_access_with_dots() {
        let expr = parse("_context.a.b.c").unwrap();
        assert_eq!(
            expr,
            Expr::FieldAccess {
                path: vec!["a".into(), "b".into(), "c".into()]
            }
        );
    }

    #[test]
    fn parse_all_comparison_ops() {
        for (op_str, op) in [
            ("==", CmpOp::Eq),
            ("!=", CmpOp::Ne),
            (">", CmpOp::Gt),
            ("<", CmpOp::Lt),
            (">=", CmpOp::Ge),
            ("<=", CmpOp::Le),
        ] {
            let expr = parse(&format!("_context.x {op_str} 42")).unwrap();
            match expr {
                Expr::Comparison { op: parsed, .. } => assert_eq!(parsed, op),
                _ => panic!("expected Comparison for {op_str}"),
            }
        }
    }

    #[test]
    fn parse_error_on_bad_field() {
        let err = parse("nope.field == 1").unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("_context"),
            "error should mention _context: {msg}"
        );
    }

    #[test]
    fn parse_error_on_trailing_tokens() {
        let err = parse("_context.x == 1 extra").unwrap_err();
        assert!(err.to_string().contains("trailing"));
    }

    // ── Evaluator tests ─────────────────────────────────────────────

    #[test]
    fn eval_eq_string() {
        let ctx = make_ctx(&[("name", serde_json::json!("alice"))]);
        let result = evaluate_expr("_context.name == \"alice\"", &ctx).unwrap();
        assert!(result);
    }

    #[test]
    fn eval_eq_string_false() {
        let ctx = make_ctx(&[("name", serde_json::json!("bob"))]);
        let result = evaluate_expr("_context.name == \"alice\"", &ctx).unwrap();
        assert!(!result);
    }

    #[test]
    fn eval_ne_string() {
        let ctx = make_ctx(&[("name", serde_json::json!("bob"))]);
        let result = evaluate_expr("_context.name != \"alice\"", &ctx).unwrap();
        assert!(result);
    }

    #[test]
    fn eval_gt_number() {
        let ctx = make_ctx(&[("score", serde_json::json!(85))]);
        let result = evaluate_expr("_context.score > 80", &ctx).unwrap();
        assert!(result);
    }

    #[test]
    fn eval_ge_number() {
        let ctx = make_ctx(&[("score", serde_json::json!(80))]);
        let result = evaluate_expr("_context.score >= 80", &ctx).unwrap();
        assert!(result);
    }

    #[test]
    fn eval_lt_number() {
        let ctx = make_ctx(&[("score", serde_json::json!(50))]);
        let result = evaluate_expr("_context.score < 60", &ctx).unwrap();
        assert!(result);
    }

    #[test]
    fn eval_le_number() {
        let ctx = make_ctx(&[("score", serde_json::json!(60))]);
        let result = evaluate_expr("_context.score <= 60", &ctx).unwrap();
        assert!(result);
    }

    #[test]
    fn eval_bool_flag() {
        let ctx = make_ctx(&[("approved", serde_json::json!(true))]);
        // Bare field access → truthy check
        let result = evaluate_expr("_context.approved", &ctx).unwrap();
        assert!(result);
    }

    #[test]
    fn eval_bool_flag_false() {
        let ctx = make_ctx(&[("approved", serde_json::json!(false))]);
        let result = evaluate_expr("_context.approved", &ctx).unwrap();
        assert!(!result);
    }

    #[test]
    fn eval_null_is_false() {
        let ctx = make_ctx(&[]);
        let result = evaluate_expr("_context.missing", &ctx).unwrap();
        assert!(!result);
    }

    #[test]
    fn eval_and_both_true() {
        let ctx = make_ctx(&[
            ("a", serde_json::json!(10)),
            ("b", serde_json::json!("yes")),
        ]);
        let result = evaluate_expr("_context.a > 5 && _context.b == \"yes\"", &ctx).unwrap();
        assert!(result);
    }

    #[test]
    fn eval_and_short_circuit() {
        let ctx = make_ctx(&[("a", serde_json::json!(3))]);
        let result = evaluate_expr("_context.a > 5 && _context.missing == \"x\"", &ctx).unwrap();
        assert!(!result);
    }

    #[test]
    fn eval_or_first_true() {
        let ctx = make_ctx(&[("a", serde_json::json!(10))]);
        let result = evaluate_expr("_context.a > 5 || _context.b", &ctx).unwrap();
        assert!(result);
    }

    #[test]
    fn eval_not() {
        let ctx = make_ctx(&[("flag", serde_json::json!(false))]);
        let result = evaluate_expr("!_context.flag", &ctx).unwrap();
        assert!(result);
    }

    #[test]
    fn eval_not_true() {
        let ctx = make_ctx(&[("flag", serde_json::json!(true))]);
        let result = evaluate_expr("!_context.flag", &ctx).unwrap();
        assert!(!result);
    }

    #[test]
    fn eval_parens_precedence() {
        let ctx = make_ctx(&[
            ("a", serde_json::json!(1)),
            ("b", serde_json::json!(2)),
            ("c", serde_json::json!(3)),
        ]);
        // (a < b) && (b < c)
        let result = evaluate_expr(
            "(_context.a < _context.b) && (_context.b < _context.c)",
            &ctx,
        )
        .unwrap();
        assert!(result);
    }

    #[test]
    fn eval_nested_path() {
        let inner = serde_json::json!({"x": 100});
        let ctx = make_ctx(&[("data", inner)]);
        let result = evaluate_expr("_context.data.x > 50", &ctx).unwrap();
        assert!(result);
    }

    #[test]
    fn eval_missing_path_is_null() {
        let ctx = make_ctx(&[]);
        let result = evaluate_expr("_context.nonexistent == null", &ctx).unwrap();
        assert!(result);
    }

    #[test]
    fn eval_type_mismatch_comparison() {
        let ctx = make_ctx(&[("name", serde_json::json!("alice"))]);
        let err = evaluate_expr("_context.name > 5", &ctx).unwrap_err();
        assert!(matches!(err, ExprError::TypeError { .. }));
    }

    #[test]
    fn eval_eq_bool() {
        let ctx = make_ctx(&[("flag", serde_json::json!(true))]);
        let result = evaluate_expr("_context.flag == true", &ctx).unwrap();
        assert!(result);
    }

    #[test]
    fn eval_complex_expression() {
        let ctx = make_ctx(&[
            ("score", serde_json::json!(85)),
            ("status", serde_json::json!("active")),
            ("reviewed", serde_json::json!(true)),
        ]);
        // (score > 80 && status == "active") || reviewed
        let result = evaluate_expr(
            "(_context.score > 80 && _context.status == \"active\") || _context.reviewed",
            &ctx,
        )
        .unwrap();
        assert!(result);
    }

    #[test]
    fn eval_number_zero_is_falsey() {
        let ctx = make_ctx(&[("count", serde_json::json!(0))]);
        let result = evaluate_expr("_context.count", &ctx).unwrap();
        assert!(!result);
    }

    #[test]
    fn eval_empty_string_is_falsey() {
        let ctx = make_ctx(&[("text", serde_json::json!(""))]);
        let result = evaluate_expr("_context.text", &ctx).unwrap();
        assert!(!result);
    }

    #[test]
    fn eval_eq_negative_number() {
        let ctx = make_ctx(&[("delta", serde_json::json!(-5))]);
        let result = evaluate_expr("_context.delta == -5", &ctx).unwrap();
        assert!(result);
    }

    #[test]
    fn eval_string_in_single_quotes() {
        let ctx = make_ctx(&[("name", serde_json::json!("alice"))]);
        let result = evaluate_expr("_context.name == 'alice'", &ctx).unwrap();
        assert!(result);
    }

    // ── Depth limit tests (V1.56 P2 fix-wave, W-003) ─────────────────

    #[test]
    fn depth_32_succeeds() {
        // Build an expression with depth exactly 32 (nested paren-paren).
        // 32 parens wrap a simple field access.
        let expr = format!("{}_context.x{}", "(".repeat(32), ")".repeat(32));
        let result = parse(&expr);
        assert!(
            result.is_ok(),
            "depth 32 should succeed, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn depth_33_fails() {
        // 33 parens should exceed MAX_EXPR_DEPTH (32).
        let expr = format!("{}_context.x{}", "(".repeat(33), ")".repeat(33));
        let result = parse(&expr);
        assert!(
            matches!(result, Err(ExprError::DepthExceeded(_))),
            "depth 33 should fail with DepthExceeded, got: {:?}",
            result
        );
    }

    #[test]
    fn depth_1000_no_panic() {
        // Extremely deep nesting should return an error, not panic.
        let expr = format!("{}_context.x{}", "(".repeat(1000), ")".repeat(1000));
        let result = parse(&expr);
        assert!(
            result.is_err(),
            "depth 1000 should produce an error, not panic"
        );
    }

    // ── Null comparison semantics (V1.56 P2 fix-wave, M-001) ─────────

    #[test]
    fn null_eq_null_is_true() {
        let ctx = make_ctx(&[]);
        let result = evaluate_expr("_context.missing == null", &ctx).unwrap();
        assert!(result, "null == null should be true (JSON semantics)");
    }

    #[test]
    fn null_eq_value_is_false() {
        let ctx = make_ctx(&[("name", serde_json::json!("alice"))]);
        let result = evaluate_expr("_context.name == null", &ctx).unwrap();
        assert!(!result, "non-null value == null should be false");
    }

    #[test]
    fn null_ne_value_is_true() {
        let ctx = make_ctx(&[("name", serde_json::json!("alice"))]);
        let result = evaluate_expr("_context.name != null", &ctx).unwrap();
        assert!(result, "non-null value != null should be true");
    }

    #[test]
    fn null_gt_zero_is_false() {
        let ctx = make_ctx(&[]);
        let err = evaluate_expr("_context.missing > 0", &ctx).unwrap_err();
        assert!(
            matches!(err, ExprError::TypeError { .. }),
            "null > 0 should be a TypeError (no numeric comparison with null)"
        );
    }

    #[test]
    fn null_ne_null_is_false() {
        let ctx = make_ctx(&[]);
        let result = evaluate_expr("_context.missing != null", &ctx).unwrap();
        assert!(!result, "null != null should be false (JSON semantics)");
    }

    // ── Context dependency scanning tests (V1.56 P3) ─────────────────

    #[test]
    fn scan_registry_source_ref() {
        let ast = parse("_context.registry_refresh.source == 'synthetic'").unwrap();
        let deps = scan_context_deps(&ast);
        assert!(deps.needs_registry_refresh);
        assert!(!deps.needs_workspace);
    }

    #[test]
    fn scan_registry_capability_count() {
        let ast = parse("_context.registry_refresh.capability_count > 50").unwrap();
        let deps = scan_context_deps(&ast);
        assert!(deps.needs_registry_refresh);
        assert!(!deps.needs_workspace);
    }

    #[test]
    fn scan_registry_fallback_reason() {
        let ast = parse("_context.registry_refresh.fallback_reason != ''").unwrap();
        let deps = scan_context_deps(&ast);
        assert!(deps.needs_registry_refresh);
        assert!(!deps.needs_workspace);
    }

    #[test]
    fn scan_workspace_conflict() {
        let ast = parse("_context.workspace.conflict_detected").unwrap();
        let deps = scan_context_deps(&ast);
        assert!(deps.needs_workspace);
        assert!(!deps.needs_registry_refresh);
    }

    #[test]
    fn scan_workspace_changes_count() {
        let ast = parse("_context.workspace.changes_applied > 0").unwrap();
        let deps = scan_context_deps(&ast);
        assert!(deps.needs_workspace);
        assert!(!deps.needs_registry_refresh);
    }

    #[test]
    fn scan_both_registry_and_workspace() {
        let ast = parse(
            "_context.registry_refresh.source == 'cdn' && _context.workspace.conflict_detected",
        )
        .unwrap();
        let deps = scan_context_deps(&ast);
        assert!(deps.needs_registry_refresh);
        assert!(deps.needs_workspace);
    }

    #[test]
    fn scan_no_deps_for_plain_expression() {
        let ast = parse("_context.score > 80").unwrap();
        let deps = scan_context_deps(&ast);
        assert!(!deps.needs_registry_refresh);
        assert!(!deps.needs_workspace);
    }

    #[test]
    fn scan_no_deps_for_simple_literal() {
        let ast = parse("true").unwrap();
        let deps = scan_context_deps(&ast);
        assert!(!deps.needs_registry_refresh);
        assert!(!deps.needs_workspace);
    }

    #[test]
    fn scan_registry_in_nested_or() {
        let ast = parse("_context.score > 80 || _context.registry_refresh.capability_count > 100")
            .unwrap();
        let deps = scan_context_deps(&ast);
        assert!(deps.needs_registry_refresh);
        assert!(!deps.needs_workspace);
    }
}
