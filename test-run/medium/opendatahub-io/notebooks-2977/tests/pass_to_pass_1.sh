#!/bin/bash
# This test must PASS on base commit AND after fix
python -m unittest -v tests.pytest_tutorial.test_01_intro.LegacyThing.test_something
