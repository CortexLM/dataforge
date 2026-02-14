#!/bin/bash
# This test must PASS on base commit AND after fix
cd /tmp/.tmpOebznq/packages/epicenter && /root/.bun/bin/bun test src/server/actions.test.ts
