#!/bin/bash
# This test must FAIL on base commit, PASS after fix
./gradlew :security:compileJava --no-daemon
