#!/bin/bash
# This test must PASS on base commit AND after fix
cd packages/visual-editor && npm run build:tsc && node --import ./tests/controller-setup.js --test-reporter=spec --test --enable-source-maps ./dist/tsc/tests/sca/actions/board/helpers/initialize-editor.test.js
