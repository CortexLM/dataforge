#!/bin/bash
# This test must PASS on base commit AND after fix
pnpm --filter=@jovie/web test -- --run --reporter=dot tests/unit/LoadingSpinner.test.tsx
