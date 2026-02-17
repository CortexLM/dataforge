#!/bin/bash
# This test must FAIL on base commit, PASS after fix
yarn install --ignore-engines --frozen-lockfile && node tests/coderabbit-config.test.js
