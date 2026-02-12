#!/usr/bin/env bash
# Run TPC-H at scale factor 100 (~600M lineitem rows).
# Requires ~32+ GB RAM. Load takes ~90 min at ~115k rows/s; then runs Q1 five times.
set -e
cd "$(dirname "$0")/.."
echo "RustafariDB TPC-H SF 100 - ensure 32+ GB RAM and 1.5–2 hours."
cargo run -p rustafari-bench --release -- tpch --scale 100 --runs 5
