#!/bin/bash
# This test must PASS on base commit AND after fix
python3 -m pytest -q tests/test_recipe_structure.py
