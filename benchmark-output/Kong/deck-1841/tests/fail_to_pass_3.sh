#!/bin/bash
# This test must FAIL on base commit, PASS after fix
cd /repo && GOTOOLCHAIN=go1.25.6 go test -v ./cmd -run "TestJSONOutput_AppendDroppedOperations"
