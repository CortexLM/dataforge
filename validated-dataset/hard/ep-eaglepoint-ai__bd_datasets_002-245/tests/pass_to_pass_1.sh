#!/bin/bash
# This test must PASS on base commit AND after fix
cd 9a9pcc-order-processing-refactor && PYTHONPATH=repository_before python -m pytest tests/test_order_processing.py -q
