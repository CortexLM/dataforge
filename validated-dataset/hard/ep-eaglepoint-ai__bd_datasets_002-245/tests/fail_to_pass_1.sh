#!/bin/bash
# This test must FAIL on base commit, PASS after fix
cd 9a9pcc-order-processing-refactor && PYTHONPATH=repository_after python -m pytest tests/test_domain_objects.py tests/test_refactoring.py tests/test_comprehensive_requirements.py -q
