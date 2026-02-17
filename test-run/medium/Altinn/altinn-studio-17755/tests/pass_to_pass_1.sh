#!/bin/bash
# This test must PASS on base commit AND after fix
yarn workspace @studio/pure-functions test --watch=false --testPathPattern FileNameUtils.test.ts
