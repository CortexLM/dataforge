#!/bin/bash
# This test must PASS on base commit AND after fix
node -e "import('./src/utils/ParseCfdiXml.mjs').then(m=>console.log(typeof m.parseCfdiXml))"
