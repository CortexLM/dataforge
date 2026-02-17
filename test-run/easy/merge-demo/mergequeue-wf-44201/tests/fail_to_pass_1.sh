#!/bin/bash
# This test must FAIL on base commit, PASS after fix
python -m unittest tests.test_word_list_updates
