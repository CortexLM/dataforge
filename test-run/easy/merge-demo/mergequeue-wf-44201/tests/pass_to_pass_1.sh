#!/bin/bash
# This test must PASS on base commit AND after fix
python -m py_compile uv/lib/delta/delta.py uv/lib/golf/golf.py
