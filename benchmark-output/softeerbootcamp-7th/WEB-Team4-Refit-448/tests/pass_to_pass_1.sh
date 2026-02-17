#!/bin/bash
# This test must PASS on base commit AND after fix
cd /repo/backend && JAVA_HOME=/usr/lib/jvm/java-17-openjdk-amd64 ./gradlew test --tests "com.shyashyashya.refit.unit.industry.*" --tests "com.shyashyashya.refit.unit.jobcategory.*" --tests "com.shyashyashya.refit.unit.global.*" --no-daemon
