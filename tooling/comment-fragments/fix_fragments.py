#!/usr/bin/env python3
"""Delete the comment fragments reported by fragments.py, comment lines only."""
import re, sys
from collections import defaultdict
sys.dont_write_bytecode = True
import fragments as F

RULE = F.RULE

def linked(cn, k, l):
    if l >= len(cn) or cn[k] is None or cn[l] is None:
        return False
    a, b = cn[k], cn[l]
    if a[0] in ("blockopen", "block") or b[0] in ("block", "blockend"):
        return a[0] in ("blockopen", "block") and b[0] in ("block", "blockend")
    return a[0] == b[0] and a[1] == b[1]

def is_frame(c):
    return c[0] == "blockend" or (c[0] == "blockopen" and not c[2])

def runs(rows):
    rows = sorted(rows)
    start = prev = None
    for r in rows:
        if prev is not None and r == prev + 1:
            prev = r
            continue
        if start is not None:
            yield start, prev
        start = prev = r
    if start is not None:
        yield start, prev

def doc_paragraph(lines, k):
    text = []
    while k < len(lines) and lines[k].strip().startswith("///") and lines[k].strip()[3:].strip():
        text.append(lines[k].strip()[3:].strip())
        k += 1
    return " ".join(text), k

def clippy_pinned(path, lines, cn, drop):
    """Deleted runs that `cargo clippy -D warnings` needs kept.

    A Rust comment that is the only body of a block keeps it from
    `clippy::needless_ifs`; a `///` summary paragraph of a public item keeps the
    paragraph after it from `clippy::too_long_first_doc_paragraph` (> 200 chars).
    """
    if not path.endswith(".rs"):
        return set()
    keep = set()
    for a, b in runs(drop):
        before = lines[a - 1].strip() if a > 0 else ""
        after = lines[b + 1].strip() if b + 1 < len(lines) else ""
        if before.endswith("{") and cn[a - 1] is None and after.startswith("}"):
            keep.update(range(a, b + 1))
            continue
        if lines[a].strip().startswith("///") and not before.startswith("///") and after.startswith("///"):
            para, k = doc_paragraph(lines, b + 1)
            while k < len(lines) and lines[k].strip().startswith(("///", "#[")):
                k += 1
            if len(para) > 200 and k < len(lines) and lines[k].strip().startswith("pub "):
                keep.update(range(a, b + 1))
    return keep

def fix(path, found):
    text = open(path, encoding="utf-8").read()
    lines = text.split("\n")
    cn = F.classify(lines, path)
    drop, closer = set(), {}
    for kind, n in sorted(found, key=lambda f: f[1]):
        if kind == "banner":
            drop.update((n - 1, n))
            continue
        if kind == "gap":
            k = n - 1
            while (k >= 0 and linked(cn, k, k + 1) and cn[k][0] != "blockopen" and cn[k][2]
                   and not F.DIRECTIVE.match(cn[k][2]) and not F.ends_sentence(cn[k][2])):
                drop.add(k)
                k -= 1
        i = n
        while True:
            if cn[i][0] == "blockend":
                closer[i] = lines[i][: cn[i][1]] + lines[i].strip()[lines[i].strip().index("*/"):]
                break
            drop.add(i)
            j = i + 1
            if not linked(cn, i, j) or not cn[j][2] or F.DIRECTIVE.match(cn[j][2]):
                break
            i = j
    done = set()
    for n in sorted(drop | set(closer)):
        if n in done:
            continue
        a, b = n, n
        while a > 0 and linked(cn, a - 1, a):
            a -= 1
        while linked(cn, b, b + 1):
            b += 1
        done.update(range(a, b + 1))
        rest = [k for k in range(a, b + 1) if k not in drop]
        body = [k for k in rest if not is_frame(cn[k]) and k not in closer]
        if all(cn[k][2] == "" or RULE.match(cn[k][2]) for k in body):
            drop.update(rest)
            for k in rest:
                closer.pop(k, None)
            continue
        prev_empty = True
        for k in body:
            empty = cn[k][2] == ""
            if empty and prev_empty:
                drop.add(k)
            prev_empty = empty
        for k in reversed(body):
            if k in drop:
                continue
            if cn[k][2] != "":
                break
            drop.add(k)
    kept = clippy_pinned(path, lines, cn, drop)
    drop -= kept
    out, dropped_since = [], False
    for k, ln in enumerate(lines):
        if k in drop:
            dropped_since = True
            continue
        ln = closer.get(k, ln)
        if dropped_since and ln.strip() == "" and out and out[-1].strip() == "":
            continue
        out.append(ln)
        dropped_since = False
    open(path, "w", encoding="utf-8").write("\n".join(out))
    return len(drop), len(closer), sorted(kept)

def main():
    base = sys.argv[1] if len(sys.argv) > 1 else "47dc4c201"
    F.BASE = base
    files = [f for f in F.git("diff", "--name-only", base, "HEAD").split("\n") if f.endswith(F.SLASH_EXT + F.HASH_EXT)]
    by = defaultdict(set)
    for f in files:
        for kind, n, p in F.analyze(f):
            by[p].add((kind, n))
    tot_d = tot_c = 0
    for p, s in by.items():
        d, c, kept = fix(p, s)
        tot_d += d; tot_c += c
        for a, b in runs(kept):
            print(f"KEPT {p}:{a + 1}-{b + 1}")
    print(f"files={len(by)} fragments={sum(map(len, by.values()))} lines_deleted={tot_d} closers_rewritten={tot_c}")

if __name__ == "__main__":
    main()
