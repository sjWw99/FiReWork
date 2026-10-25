#!/usr/bin/env bash
set -e
echo "Building FiReWork.exe (Rust release, windows-gnu)..."
cargo build --release --target x86_64-pc-windows-gnu
cp -f target/x86_64-pc-windows-gnu/release/FiReWork.exe ./FiReWork.exe
echo "Done: FiReWork.exe"
