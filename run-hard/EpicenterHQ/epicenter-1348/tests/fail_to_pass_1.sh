#!/bin/bash
# This test must FAIL on base commit, PASS after fix
cd /tmp/.tmpOebznq/packages/server && /root/.bun/bin/bun test
