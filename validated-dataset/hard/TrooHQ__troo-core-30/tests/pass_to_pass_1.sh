#!/bin/bash
# This test must PASS on base commit AND after fix
DJANGO_SETTINGS_MODULE=test_settings PYTHONPATH=app:/tmp pytest -q app/apps/merchants/tests/test_api_views.py::MerchantsAPITestCase::test_merchant_list_requires_scope -q
