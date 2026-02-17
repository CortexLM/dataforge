#!/bin/bash
# This test must FAIL on base commit, PASS after fix
cd frontend && pnpm exec jest src/scenes/max/__tests__/threadDataVisualizationLimitContext.test.ts --runTestsByPath
