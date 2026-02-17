#!/bin/bash
# This test must PASS on base commit AND after fix
cd /repo/python_client && python -c "import kubetorch; print('kubetorch imported successfully')"
