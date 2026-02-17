#!/bin/bash
# This test must PASS on base commit AND after fix
npm run lint:js:cli -- "eleventy/shortcodes/link-card.js"
