#!/bin/bash
# This test must FAIL on base commit, PASS after fix
cargo run -p openviking-cli -- ls --json
