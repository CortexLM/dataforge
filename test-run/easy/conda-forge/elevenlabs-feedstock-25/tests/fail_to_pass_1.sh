#!/bin/bash
# This test must FAIL on base commit, PASS after fix
python3 -m pytest -q tests/test_recipe_update.py
