#!/bin/bash
# This test must PASS on base commit AND after fix
npm test -- --runTestsByPath src/__tests__/assets.route.test.ts
