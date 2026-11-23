//! Comment-blind Rust token comparison for a comment-only change.
//!
//! Each file pair (old, new) is lexed with `proc_macro2`, which drops `//` and
//! `/* */` comments; `#[doc]` attributes (doc comments) are dropped too. Two
//! layout choices rustfmt makes once a comment no longer pins them are then
//! normalised on both sides: the order inside a run of `use`/`mod x;` items, a
//! single-expression block body of a match arm or closure versus the bare
//! expression, and a trailing comma before a closing delimiter.
use proc_macro2::{Delimiter, TokenStream, TokenTree};
use std::str::FromStr;

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum T {
    Leaf(String),
    Group(char, Vec<T>),
}

fn leaf(t: &T, s: &str) -> bool {
    matches!(t, T::Leaf(x) if x == s)
}

fn lower(ts: TokenStream) -> Vec<T> {
    let toks: Vec<TokenTree> = ts.into_iter().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < toks.len() {
        if let TokenTree::Punct(p) = &toks[i] {
            if p.as_char() == '#' {
                let mut j = i + 1;
                if matches!(toks.get(j), Some(TokenTree::Punct(q)) if q.as_char() == '!') {
                    j += 1;
                }
                if let Some(TokenTree::Group(g)) = toks.get(j) {
                    let is_doc = g.delimiter() == Delimiter::Bracket
                        && matches!(g.stream().into_iter().next(), Some(TokenTree::Ident(id)) if id == "doc");
                    if is_doc {
                        i = j + 1;
                        continue;
                    }
                }
            }
        }
        out.push(match &toks[i] {
            TokenTree::Group(g) => {
                let d = match g.delimiter() {
                    Delimiter::Brace => '{',
                    Delimiter::Bracket => '[',
                    Delimiter::Parenthesis => '(',
                    Delimiter::None => '_',
                };
                T::Group(d, lower(g.stream()))
            }
            t => T::Leaf(t.to_string()),
        });
        i += 1;
    }
    out
}

/// End (exclusive) of a `#[..]* pub(..)? use ...;` or `mod name;` item starting at `i`.
fn simple_item(v: &[T], i: usize) -> Option<usize> {
    let mut j = i;
    while leaf(v.get(j)?, "#") && matches!(v.get(j + 1), Some(T::Group('[', _))) {
        j += 2;
    }
    if leaf(v.get(j)?, "pub") {
        j += 1;
        if matches!(v.get(j), Some(T::Group('(', _))) {
            j += 1;
        }
    }
    if leaf(v.get(j)?, "use") {
        (j..v.len()).find(|&k| leaf(&v[k], ";")).map(|k| k + 1)
    } else if leaf(v.get(j)?, "mod") && leaf(v.get(j + 2)?, ";") {
        Some(j + 3)
    } else {
        None
    }
}

fn normalize(v: Vec<T>) -> Vec<T> {
    let v: Vec<T> = v
        .into_iter()
        .map(|t| match t {
            T::Group(d, inner) => T::Group(d, normalize(inner)),
            l => l,
        })
        .collect();
    let mut arms = Vec::with_capacity(v.len());
    let mut i = 0;
    while i < v.len() {
        let arm = leaf(&v[i], "=") && v.get(i + 1).is_some_and(|t| leaf(t, ">"));
        let closure = !arm && leaf(&v[i], "|");
        let head = if arm { 2 } else { 1 };
        if arm || closure {
            if let Some(T::Group('{', inner)) = v.get(i + head) {
                if !inner.iter().any(|t| leaf(t, ";") || leaf(t, "let")) && !inner.is_empty() {
                    arms.extend(v[i..i + head].iter().cloned());
                    arms.extend(inner.iter().cloned());
                    i += head + 1;
                    if arm {
                        arms.push(T::Leaf(",".into()));
                        if v.get(i).is_some_and(|t| leaf(t, ",")) {
                            i += 1;
                        }
                    }
                    continue;
                }
            }
        }
        arms.push(v[i].clone());
        i += 1;
    }
    let mut out = Vec::with_capacity(arms.len());
    let mut i = 0;
    while i < arms.len() {
        let mut run = Vec::new();
        let mut j = i;
        while let Some(end) = simple_item(&arms, j) {
            run.push(arms[j..end].to_vec());
            j = end;
        }
        if run.is_empty() {
            out.push(arms[i].clone());
            i += 1;
        } else {
            run.sort();
            out.extend(run.into_iter().flatten());
            i = j;
        }
    }
    if out.last().is_some_and(|t| leaf(t, ",")) {
        out.pop();
    }
    out
}

fn flatten(v: &[T]) -> Vec<String> {
    let mut out = Vec::new();
    for t in v {
        match t {
            T::Leaf(s) => out.push(s.clone()),
            T::Group(d, inner) => {
                out.push(d.to_string());
                out.extend(flatten(inner));
                out.push("~".into());
            }
        }
    }
    out
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut bad = 0;
    for pair in args.chunks(2) {
        let side = |p: &str| {
            let src = std::fs::read_to_string(p).expect("read");
            normalize(lower(TokenStream::from_str(&src).expect("lex")))
        };
        let (a, b) = (side(&pair[0]), side(&pair[1]));
        if a != b {
            println!("DIFF {}", pair[1]);
            if std::env::var_os("RSTOK_DEBUG").is_some() {
                let (fa, fb) = (flatten(&a), flatten(&b));
                let k = fa.iter().zip(&fb).take_while(|(x, y)| x == y).count();
                println!(
                    "  old: {}\n  new: {}",
                    fa[k.saturating_sub(8)..(k + 12).min(fa.len())].join(" "),
                    fb[k.saturating_sub(8)..(k + 12).min(fb.len())].join(" ")
                );
            }
            bad += 1;
        }
    }
    println!("rust_files={} token_mismatch={bad}", args.len() / 2);
    std::process::exit(i32::from(bad > 0));
}
