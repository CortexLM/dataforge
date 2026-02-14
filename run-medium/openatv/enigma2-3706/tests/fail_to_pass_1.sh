#!/bin/bash
# This test must FAIL on base commit, PASS after fix
pytest -q /tmp/test_softcsa_noaudio.py
