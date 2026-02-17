#!/bin/bash
# This test must FAIL on base commit, PASS after fix
npm test -- --run tests/unit/core-byok-only.test.ts
