#!/bin/bash
# This test must PASS on base commit AND after fix
cd /repo && GOTOOLCHAIN=auto go test ./internal/release/ -v -count=1
