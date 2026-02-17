#!/bin/bash
# This test must PASS on base commit AND after fix
python -m pytest tests/unit/package_managers/pip/test_main.py::test_fetch_pip_source -q
