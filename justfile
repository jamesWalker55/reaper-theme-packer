# Usage:
# just test preprocess::tests::test_01
test module:
    RUST_BACKTRACE=1 cargo test {{module}} -- --exact --nocapture
