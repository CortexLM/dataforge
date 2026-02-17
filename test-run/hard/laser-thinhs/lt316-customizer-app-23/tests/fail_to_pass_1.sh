#!/bin/bash
# This test must FAIL on base commit, PASS after fix
npm test -- --runTestsByPath src/__tests__/tracing-core-settings.test.ts src/__tests__/tracing-core-error.test.ts
