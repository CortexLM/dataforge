#!/bin/bash
# This test must FAIL on base commit, PASS after fix
pnpm --filter landing exec vitest --run src/components/providers/starknet-connectors.test.ts
