#!/bin/bash
# This test must PASS on base commit AND after fix
./gradlew :security:compileJava --no-daemon -q
