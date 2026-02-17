#!/bin/bash
# This test must FAIL on base commit, PASS after fix
cd /repo/Dalum-BE && ./gradlew test --tests "dalum.dalum.global.s3.S3ServiceTest" --no-daemon
