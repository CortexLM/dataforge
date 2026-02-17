#!/bin/bash
# This test must FAIL on base commit, PASS after fix
npx tsx tests/sms-template-send-to-all-extra-phones.test.ts
