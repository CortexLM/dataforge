#!/bin/bash
# This test must FAIL on base commit, PASS after fix
python -m unittest -v tests.test_aipcc_copr_unittest
