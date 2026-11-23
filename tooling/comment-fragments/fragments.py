#!/usr/bin/env python3
"""Detect comment fragments left by a comment-deletion sweep.

Compares each file at BASE (before the sweep) with the working tree and reports
comment runs whose surviving lines are a partial sentence:
  head: a comment block now starts on a line whose deleted predecessor in the
        same original block did not end a sentence;
  gap:  a block keeps lines on both sides of a deleted span that broke a sentence;
  banner: two separator lines are now adjacent because the title between them
          was deleted.
Separator-only lines and directives (lint/type pragmas, license headers,
`@generated`, `SAFETY:`) are never reported.
"""
import difflib, json, re, subprocess, sys

BASE = sys.argv[1] if len(sys.argv) > 1 else "47dc4c201"
MODE = sys.argv[2] if len(sys.argv) > 2 else "report"   # report | list | json
SELF = "tooling/comment-fragments/"
SLASH_EXT = (".rs", ".ts", ".tsx", ".mjs", ".js", ".mts", ".cjs", ".jsx", ".template")
HASH_EXT = (".sh",)

def git(*a):
    return subprocess.run(["git", *a], capture_output=True, text=True, check=True).stdout

def _classify(lines, path):
    """Per line: None for non-comment, else (kind, indent, text)."""
    out, in_block = [], False
    slash = path.endswith(SLASH_EXT)
    for ln in lines:
        s = ln.strip()
        ind = len(ln) - len(ln.lstrip())
        if slash:
            if in_block:
                if "*/" in s:
                    in_block = False
                    t = s.split("*/")[0].lstrip("*").strip()
                    out.append(("blockend", ind, t))
                else:
                    out.append(("block", ind, s.lstrip("*").strip()))
                continue
            if s.startswith("/*") or s.startswith("{/*"):
                body = s[3:] if s.startswith("{/*") else s[2:]
                if "*/" not in body:
                    in_block = True
                    out.append(("blockopen", ind, body.lstrip("*!").strip()))
                else:
                    out.append(None)
                continue
            for k in ("///", "//!", "//"):
                if s.startswith(k):
                    out.append((k, ind, s[len(k):].strip()))
                    break
            else:
                out.append(None)
        else:
            if s.startswith("#") and not s.startswith("#!"):
                out.append(("#", ind, s[1:].strip()))
            else:
                out.append(None)
    return out

def string_lines(src, path):
    """Indices of lines that begin inside a string literal (Rust or JS/TS)."""
    rust = path.endswith(".rs")
    inside, i, n, line = set(), 0, len(src), 0
    state, close, tmpl = None, None, []
    while i < n:
        ch = src[i]
        if ch == "\n":
            line += 1
            if state in ("str", "raw", "tmpl"):
                inside.add(line)
            elif state == "line":
                state = None
            i += 1
            continue
        if state == "line":
            i += 1
        elif state == "block":
            if src.startswith("*/", i):
                state, i = None, i + 2
            else:
                i += 1
        elif state == "str":
            if ch == "\\":
                i += 2 if i + 1 < n and src[i + 1] != "\n" else 1
            elif ch == close:
                state, i = None, i + 1
            else:
                i += 1
        elif state == "raw":
            if src.startswith(close, i):
                state, i = None, i + len(close)
            else:
                i += 1
        elif state == "tmpl":
            if ch == "\\":
                i += 2
            elif ch == "`":
                state, i = None, i + 1
            elif src.startswith("${", i):
                tmpl.append(0); state, i = None, i + 2
            else:
                i += 1
        else:
            if src.startswith("//", i):
                state, i = "line", i + 2
            elif src.startswith("/*", i):
                state, i = "block", i + 2
            elif rust and ch == "r" and re.match(r'r#*"', src[i:i + 260]) and not (i and (src[i - 1].isalnum() or src[i - 1] == "_")):
                m = re.match(r'r(#*)"', src[i:]); close = '"' + m.group(1); state, i = "raw", i + m.end()
            elif ch == '"':
                state, close, i = "str", '"', i + 1
            elif ch == "'":
                if rust:
                    m = re.match(r"'(\\(u\{[0-9a-fA-F]+\}|x[0-9a-fA-F]{2}|.)|[^\\'])'", src[i:i + 16])
                    i += m.end() if m else 1
                else:
                    state, close, i = "str", "'", i + 1
            elif not rust and ch == "`":
                state, i = "tmpl", i + 1
            elif not rust and tmpl and ch == "{":
                tmpl[-1] += 1; i += 1
            elif not rust and tmpl and ch == "}":
                if tmpl[-1] == 0:
                    tmpl.pop(); state = "tmpl"
                else:
                    tmpl[-1] -= 1
                i += 1
            else:
                i += 1
    return inside

def classify(lines, path):
    out = _classify(lines, path)
    if path.endswith(SLASH_EXT):
        for k in string_lines("\n".join(lines), path):
            if k < len(out):
                out[k] = None
    return out

def same_block(a, b):
    if a is None or b is None:
        return False
    ka, kb = a[0], b[0]
    if ka.startswith("block") or kb.startswith("block"):
        return ka in ("blockopen", "block") and kb in ("block", "blockend")
    return ka == kb and a[1] == b[1]

END = re.compile(r"""([.!?:;。！？：]|[.!?]["'`)\]]|```|\*/)$""")
DIRECTIVE = re.compile(r"^(eslint-|oxlint-|@ts-|prettier-ignore|biome-ignore|SAFETY:|Safety:|@generated|c8 |istanbul |#region|#endregion|shellcheck |TODO|FIXME|@vite-ignore|webpackChunkName|webpackIgnore|<reference |@vitest-|@jest-|@jsx|[#@]__PURE__|@license|@preserve|SPDX-|Copyright|cspell:|deno-|language=)")
RULE = re.compile(r"^[^\w\s`'\"]{3,}$")

HEADING = re.compile(r"^[^\w\s`'\"(]{2,}.*[^\w\s`'\")]{2,}$")

def ends_sentence(text):
    return text == "" or bool(END.search(text) or RULE.match(text) or HEADING.match(text))

def starts_continuation(text):
    return bool(text) and (text[0].islower() or text[0] in "),;—–&|") and not text.startswith(("- ", "* "))

def analyze(path):
    try:
        old = git("show", f"{BASE}:{path}").splitlines()
    except subprocess.CalledProcessError:
        return []
    try:
        new = open(path, encoding="utf-8").read().splitlines()
    except FileNotFoundError:
        return []
    co, cn = classify(old, path), classify(new, path)
    new2old = {}
    sm = difflib.SequenceMatcher(None, old, new, autojunk=False)
    for tag, i1, i2, j1, j2 in sm.get_opcodes():
        if tag == "equal":
            for k in range(i2 - i1):
                new2old[j1 + k] = i1 + k
    kept_old = set(new2old.values())
    found = []
    for n, c in enumerate(cn):
        if c is None or not c[2]:
            continue
        o = new2old.get(n)
        if o is None or DIRECTIVE.match(c[2]):
            continue
        prev_new = n - 1
        pn = cn[prev_new] if prev_new >= 0 else None
        if RULE.match(c[2]):
            # banner: two separator lines now adjacent because the title between them was deleted
            po = new2old.get(prev_new)
            if (pn is not None and same_block(pn, c) and RULE.match(pn[2]) and po is not None
                    and po < o - 1 and any(k not in kept_old for k in range(po + 1, o))):
                found.append(("banner", n, path))
            continue
        # head: no surviving same-block content line before this one
        p = o - 1
        if p >= 0 and same_block(co[p], co[o]) and p not in kept_old:
            prev_is_opener_only = pn is not None and pn[0] == "blockopen" and not pn[2]
            if not same_block(pn, c) or prev_is_opener_only:
                if not ends_sentence(co[p][2]):
                    found.append(("head", n, path))
                    continue
            elif new2old.get(prev_new) is not None and new2old[prev_new] < p:
                if not ends_sentence(co[p][2]) or starts_continuation(c[2]):
                    found.append(("gap", n, path))
                    continue
    return found

def main():
    files = [f for f in git("diff", "--name-only", BASE, "HEAD").split("\n")
             if f.endswith(SLASH_EXT + HASH_EXT) and not f.startswith(SELF)]
    allf = []
    for f in files:
        allf += analyze(f)
    if MODE == "json":
        json.dump(allf, sys.stdout)
        return
    if MODE == "list":
        for kind, n, p in sorted(allf, key=lambda f: (f[2], f[1])):
            print(f"{p}:{n + 1} {kind}")
    from collections import Counter
    c = Counter(k for k, _, _ in allf)
    print(f"base={BASE} files_scanned={len(files)} fragments={len(allf)} "
          f"files_with_fragments={len({p for _, _, p in allf})} " + " ".join(f"{k}={v}" for k, v in sorted(c.items())))

if __name__ == "__main__":
    main()

