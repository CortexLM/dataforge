#!/bin/bash
# This test must PASS on base commit AND after fix
cd /repo && GOTOOLCHAIN=auto go test ./pkg/app/plugin/... -v
