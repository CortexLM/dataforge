#!/bin/bash
# This test must PASS on base commit AND after fix
npx tsx tests/sms-template-send-bulk-existing.test.ts
