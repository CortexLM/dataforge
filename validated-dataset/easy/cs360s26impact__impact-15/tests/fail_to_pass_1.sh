#!/bin/bash
# This test must FAIL on base commit, PASS after fix
javac app/src/main/java/edu/lums/impact/*.java RectangleBehaviorTest.java && java -cp .:app/src/main/java RectangleBehaviorTest
