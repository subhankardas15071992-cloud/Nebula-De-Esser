#!/bin/bash
# Nebula DeEsser Build Verification Script
# This script verifies the build and runs tests

set -e

echo "========================================="
echo "Nebula DeEsser Build Verification"
echo "========================================="
echo ""

# Check for required tools
echo "Checking build environment..."
if ! command -v cargo &> /dev/null; then
    echo "ERROR: Rust/Cargo not found. Please install Rust first."
    exit 1
fi

echo "1. Running cargo check..."
cargo check
if [ $? -eq 0 ]; then
    echo "✓ Cargo check passed"
else
    echo "✗ Cargo check failed"
    exit 1
fi

echo ""
echo "2. Running tests..."
cargo test --lib -- --test-threads=1
if [ $? -eq 0 ]; then
    echo "✓ All tests passed"
else
    echo "✗ Tests failed"
    exit 1
fi

echo ""
echo "3. Running clippy (linter)..."
cargo clippy -- -D warnings
if [ $? -eq 0 ]; then
    echo "✓ Clippy passed (no warnings)"
else
    echo "✗ Clippy found issues"
    exit 1
fi

echo ""
echo "4. Checking formatting..."
cargo fmt -- --check
if [ $? -eq 0 ]; then
    echo "✓ Code is properly formatted"
else
    echo "✗ Code formatting issues found"
    exit 1
fi

echo ""
echo "5. Building in release mode..."
cargo build --release
if [ $? -eq 0 ]; then
    echo "✓ Build successful"
else
    echo "✗ Build failed"
    exit 1
fi

echo ""
echo "========================================="
echo "✓ All checks passed! Nebula DeEsser is ready."
echo "Build artifacts in: target/release/"
echo "CLAP bundle will be in: target/bundled/"
echo "========================================="