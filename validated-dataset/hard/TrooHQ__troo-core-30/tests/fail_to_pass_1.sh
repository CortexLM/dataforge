#!/bin/bash
# This test must FAIL on base commit, PASS after fix
DJANGO_SETTINGS_MODULE=test_settings PYTHONPATH=app:/tmp pytest -q app/apps/merchants/tests/test_stations_locations_views.py -q
