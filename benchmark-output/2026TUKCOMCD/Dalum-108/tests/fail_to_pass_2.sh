#!/bin/bash
# This test must FAIL on base commit, PASS after fix
cd /repo/Dalum-BE && ./gradlew test --tests "dalum.dalum.domain.dupe_product.controller.DupeProductControllerTest" --no-daemon
