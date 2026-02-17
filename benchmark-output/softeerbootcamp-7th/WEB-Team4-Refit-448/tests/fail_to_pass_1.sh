#!/bin/bash
# This test must FAIL on base commit, PASS after fix
cd /repo/backend && JAVA_HOME=/usr/lib/jvm/java-17-openjdk-amd64 ./gradlew test --tests "com.shyashyashya.refit.unit.interview.dto.InterviewDtoIndustryTest" --no-daemon
