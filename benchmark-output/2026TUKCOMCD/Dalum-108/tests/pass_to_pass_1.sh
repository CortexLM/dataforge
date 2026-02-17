#!/bin/bash
# This test must PASS on base commit AND after fix
cd /repo/Dalum-BE && ./gradlew test --tests "dalum.dalum.domain.like_product.service.LikeProductServiceTest" --no-daemon
