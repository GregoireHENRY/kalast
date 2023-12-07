#!/bin/bash

echo $PLATFORM

echo "Build main executable."
cargo build -r --all-features && strip target/release/kalast

echo "Build custom executable for specific examples." 
cargo build -r --example --all-features viewer-picker && strip target/release/examples/viewer-picker