#!/bin/bash
# This test must FAIL on base commit, PASS after fix
./gradlew :core:compileJava --no-daemon
