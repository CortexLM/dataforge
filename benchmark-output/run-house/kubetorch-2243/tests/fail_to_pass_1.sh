#!/bin/bash
# This test must FAIL on base commit, PASS after fix
cd /repo/python_client && python -c "from tests.test_remote_dir import TestModuleRemoteDir, TestClsRemoteDir, TestFnRemoteDir; t = TestModuleRemoteDir(); t.test_module_accepts_remote_dir_parameter(); print('PASS')"
