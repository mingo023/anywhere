#!/bin/bash
# Window-only screenshot without stealing focus. usage: ./shot.sh <name> [font: system|geist] [scroll-px]
cd "$(dirname "$0")"
[ -x winid ] || swiftc -O winid.swift -o winid
BIN=${CARGO_TARGET_DIR:-/tmp/pocket-spike/target}/release/mermaid-spike
SPIKE_COPIES=1 SPIKE_FONT=${2:-system} SPIKE_SCROLL=${3:-0} $BIN > /dev/null 2>&1 & PID=$!
sleep 4
screencapture -l "$(./winid)" -o -x "$1.png"
kill $PID
