#!/bin/bash
# This test must PASS on base commit AND after fix
/tmp/venv/bin/python -m pytest -q tests/test_filelock.py::test_lock_mode_soft
