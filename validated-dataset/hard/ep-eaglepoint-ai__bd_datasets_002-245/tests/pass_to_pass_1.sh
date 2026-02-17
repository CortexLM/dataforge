#!/bin/bash
# This test must PASS on base commit AND after fix
cd j1p4vf-ecommerce-order-processing-refactor && npm test -- --runTestsByPath tests/refactored.test.js
