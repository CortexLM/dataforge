#!/bin/bash
# This test must PASS on base commit AND after fix
PYTHONPATH=/repo python3 tests/test_existing_functionality.py
