---
title: An immutability assertion must see every mutation it claims to forbid
slug: immutability-assertions-need-lossless-comparison
kind: how-to
status: active
applies_to: [testing, sqlite, storage]
---

# An immutability assertion must see every mutation it claims to forbid

A test that asserts "this row did not change" is only as strong as its comparison. Three ways a well-intentioned snapshot silently misses a real mutation, each found while hardening one such test:

## `quote()` truncates text at an embedded NUL

```sql
json_array(quote('existing_entry'))                          -- ["'existing_entry'"]
json_array(quote('existing_entry' || char(0) || 'changed'))  -- ["'existing_entry'"]
```

Identical output, different bytes. A column that gains a suffix after a NUL passes the comparison.

## `CAST(REAL AS BLOB)` converts through decimal text

Two doubles that differ past 15 significant digits render identically:

```sql
-- both produce ["real","312E30"]  ("1.0")
CAST(1.000000000000001 AS BLOB)
CAST(1.000000000000002 AS BLOB)
```

A non-`STRICT` SQLite table accepts a double in an `INTEGER`-declared column, so this is reachable without a schema violation.

## Enumerating columns by hand misses the ones you did not think of

The first version of the test compared seven hand-picked columns. The table had 22, including owner and governance columns nobody had reason to consider.

## The form that holds

Per column, from `pragma_table_info`, ordered by primary key:

```sql
json_array(
  typeof(col),
  CASE typeof(col) WHEN 'real' THEN printf('%!.17g', col)
                   ELSE hex(CAST(col AS BLOB)) END
)
```

- `typeof` separates `NULL` from `''`, and a BLOB from its text spelling.
- `%!.17g` round-trips every double; `CAST` would truncate.
- `hex(CAST(… AS BLOB))` is the raw bytes for everything else, so no text conversion touches it.
- Reading the column list from the schema means a migration moves the baseline with it — correct for an immutability test, wrong for a schema-pinning one. Say which you want.

## The negative control decides whether any of it is real

A comparison test passes trivially if the thing under test is never exercised. Prove it fails: revert the mechanism the assertion protects and confirm the **comparison** is what reds, not an earlier weaker assertion in the same body. Ordering matters — put the strongest assertion first, or the negative control stops at something that would have passed anyway.

## Cost note

This took four reviewer rounds, each closing a real hole. That is the correct cost for a primitive that will be reused; it is a poor trade for a one-off test of a path nothing production reaches. Decide which you are writing before the third round.
