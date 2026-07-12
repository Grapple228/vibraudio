#!/bin/bash

run_tests() {
    local features="$1"
    echo "Running tests with features: $features"
    cargo test --release --no-default-features --features "$features"
    
    # Check if test was successfull
    if [ $? -ne 0 ]; then
        echo "Tests failed for features: $features"
        exit 1
    fi
}

run_tests "feature"

# Test all
echo "Running tests with all features"
cargo test --all-features

echo "All tests completed successfully."