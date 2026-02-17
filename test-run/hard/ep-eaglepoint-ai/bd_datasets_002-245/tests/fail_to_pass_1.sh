#!/bin/bash
# This test must FAIL on base commit, PASS after fix
cd j1p4vf-ecommerce-order-processing-refactor && npm test -- --runTestsByPath tests/refactor-additional.test.js
