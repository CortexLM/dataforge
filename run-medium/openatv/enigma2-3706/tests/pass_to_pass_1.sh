#!/bin/bash
# This test must PASS on base commit AND after fix
pytest -q /tmp/test_repo_sanity.py
