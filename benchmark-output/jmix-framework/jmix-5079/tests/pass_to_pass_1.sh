#!/bin/bash
# This test must PASS on base commit AND after fix
./gradlew :core:compileJava --no-daemon -q
