#!/bin/bash
# This test must FAIL on base commit, PASS after fix
PYTHONPATH=/repo python3 tests/test_toml_config_system.py
