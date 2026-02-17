#!/bin/bash
# This test must FAIL on base commit, PASS after fix
python -m pytest tests/test_yquake2_riscv_config.py -q
