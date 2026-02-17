#!/bin/bash
# This test must PASS on base commit AND after fix
pnpm --filter landing exec vitest --run src/lib/pagination.test.ts
