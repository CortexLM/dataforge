#!/bin/bash
# This test must FAIL on base commit, PASS after fix
python -m unittest tests.test_readme_steps
