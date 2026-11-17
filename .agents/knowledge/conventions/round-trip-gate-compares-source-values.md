# A round-trip gate must compare the source representation, not the typed form

## Symptom

`preset export` and `preset import` looked symmetric, the types were `Option`, and a typed comparison passed —
yet nine of twenty embedded presets lost data on a round trip. The probe also showed the inverse: an omitted
`body:` came back as `body: ""`.

## Cause

The typed manifest folds two different source shapes into one Rust value:

- `ContextUpdateOp::Append { body: Option<String> }` — absent and `null` both become `None`; `Some("")` is the
  only empty form.
- Re-parsing an *exported* YAML into the typed manifest turns a missing `body:` into `None` again, so
  `typed_exported == typed_source` even when the export invented `body: ""` or dropped a key.

A gate that compares typed manifests therefore cannot see key-level loss. It compares a lossy projection with
itself.

## Rule

Compare the **source representation** (parsed YAML/JSON values, key sets included), keyed by the paths the
contract names:

```rust
let source: serde_yaml::Value = serde_yaml::from_str(&source_yaml)?;
let exported: serde_yaml::Value = serde_yaml::from_str(&serde_yaml::to_string(&exported_manifest)?)?;
assert_eq!(named_fields(&source), named_fields(&exported));
```

`serde_yaml::Mapping` equality is order-insensitive, so only key sets and values matter.

## Corollary: pin the shapes the corpus does not contain

The embedded corpus carried no payload at all, so a gate over the corpus alone could not catch either direction.
The gate needs an inline fixture enumerating the states — absent, explicit null, empty string, non-empty — and
each state needs a named assertion besides the whole-block equality. Without that, a fixture edit can silently
drop a state and the gate stays green.

## Corollary: `Option<T>` is one bit, `Option<Option<T>>` is two

An explicit `null` and an absent key are different source values. `Option<T>` collapses them; preserving the
distinction needs `Option<Option<T>>` with a `deserialize_with` that maps `null` to `Some(None)` and an omitted
key to `None`, plus `skip_serializing_if = "Option::is_none"` on the outer option.
