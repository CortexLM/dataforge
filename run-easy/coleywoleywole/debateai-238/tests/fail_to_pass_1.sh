#!/bin/bash
# This test must FAIL on base commit, PASS after fix
npx vitest run tests/daily-debates.test.ts
