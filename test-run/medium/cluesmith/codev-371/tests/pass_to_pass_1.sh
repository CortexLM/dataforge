#!/bin/bash
# This test must PASS on base commit AND after fix
cd packages/codev && npm test -- src/__tests__/bugfix-280-consult-diff.test.ts
