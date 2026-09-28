#!/bin/bash
# Window-only screenshot without stealing focus. usage: ./shot.sh <tab>
cd "$(dirname "$0")"
[ -x winid ] || swiftc -O winid.swift -o winid
BIN=${CARGO_TARGET_DIR:-/tmp/pocket-spike}/release/pocket-spike
SPIKE_TAB=$1 $BIN > /dev/null 2>&1 & PID=$!
sleep 4
screencapture -l "$(./winid)" -o -x "$1.png"
kill $PID
