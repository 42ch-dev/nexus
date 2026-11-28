// Fixture: a multi-line test-gated macro body is delimited by brace depth, so
// wrong indentation does not end the span at the wrong `}`.
#[cfg(test)]
paste::paste! {
    mod generated {
        fn write_fixture(path: &str, bytes: &[u8]) {
            std::fs::write(path, bytes).unwrap();
        }
    }
    }
