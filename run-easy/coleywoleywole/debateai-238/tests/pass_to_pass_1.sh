#!/bin/bash
# This test must PASS on base commit AND after fix
npx vitest run tests/api-schemas.test.ts
