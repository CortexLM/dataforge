#!/bin/bash
# This test must FAIL on base commit, PASS after fix
yarn workspace @studio/pure-functions test --watch=false --testPathPattern PublishedElements.export.test.ts
