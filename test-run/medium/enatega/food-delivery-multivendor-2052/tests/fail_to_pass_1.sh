#!/bin/bash
# This test must FAIL on base commit, PASS after fix
cd enatega-multivendor-app && node tests/faqs-accordion.test.js
