#!/bin/bash
# usage: ./bench.sh <px-per-frame|open> [copies] [font: system|geist]
# Build first: CARGO_TARGET_DIR=/tmp/pocket-spike/target cargo build --release (Cargo.lock is a copy of packages/desktop's)
BIN=${CARGO_TARGET_DIR:-/tmp/pocket-spike/target}/release/mermaid-spike
export SPIKE_POPUP=1
[ "$1" = open ] || export SPIKE_BENCH=$1
SPIKE_COPIES=${2:-10} SPIKE_FONT=${3:-system} $BIN > /tmp/mermaid-spike.log 2>&1 & PID=$!
for i in $(seq 1 ${SECS:-12}); do sleep 1; kill -0 $PID 2>/dev/null || break; done
kill $PID 2>/dev/null
grep -E "^\[(time|input|open|bench|work|lazy|reload)\]" /tmp/mermaid-spike.log
