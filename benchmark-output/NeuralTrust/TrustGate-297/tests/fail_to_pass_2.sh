#!/bin/bash
# This test must FAIL on base commit, PASS after fix
cd /repo && GOTOOLCHAIN=auto go test ./pkg/version/... -run TestVersionUpdate -v
