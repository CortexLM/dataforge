#!/bin/bash
# This test must PASS on base commit AND after fix
pytest -q tests/test_config_manager.py -q
