#!/bin/bash
# Scroll benchmark. usage: ./bench.sh <editor|rows|md|diff> <px-per-frame> [source-lines]
# Build first: CARGO_TARGET_DIR=/tmp/pocket-spike cargo build --release (copy packages/desktop/Cargo.lock here to pin versions)
BIN=${CARGO_TARGET_DIR:-/tmp/pocket-spike}/release/pocket-spike
SPIKE_TAB=$1 SPIKE_BENCH=$2 SPIKE_LINES=${3:-20000} $BIN > /tmp/spike-$1.log 2>&1 & PID=$!
for i in $(seq 1 40); do sleep 1; kill -0 $PID 2>/dev/null || break; done
kill $PID 2>/dev/null
grep -E "^\[(time|input|bench|work)\]" /tmp/spike-$1.log
