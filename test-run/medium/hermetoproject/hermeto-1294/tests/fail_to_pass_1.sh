#!/bin/bash
# This test must FAIL on base commit, PASS after fix
python -m pytest tests/unit/package_managers/pip/test_fetch_pip_source_env.py -q
