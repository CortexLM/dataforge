#!/bin/bash
# This test must PASS on base commit AND after fix
npx vitest run src/App.test.tsx -t "tests API connection with saved base url and token"
