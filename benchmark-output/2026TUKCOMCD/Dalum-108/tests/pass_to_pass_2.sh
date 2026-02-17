#!/bin/bash
# This test must PASS on base commit AND after fix
cd /repo/Dalum-BE && ./gradlew test --tests "dalum.dalum.domain.search_log.service.SearchLogServiceTest" --no-daemon
