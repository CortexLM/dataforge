#!/bin/bash
# This test must FAIL on base commit, PASS after fix
cd packages/codev && npm install --ignore-scripts && npm test -- src/__tests__/gemini-yolo.test.ts
