#!/bin/bash
# This test must PASS on base commit AND after fix
/tmp/venv/bin/python -m pytest -q tests/test_read_write_unit.py::test_lock_context_manager
