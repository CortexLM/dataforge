#!/bin/bash
# This test must FAIL on base commit, PASS after fix
node --experimental-vm-modules ./node_modules/.bin/jest tests/logs-controller.test.mjs
