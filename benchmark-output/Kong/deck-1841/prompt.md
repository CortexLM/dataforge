# Kong/deck-1841

Fix the JSON summary output to accurately reflect operations performed on the gateway when upstream errors occur. Currently, the JSON summary incorrectly shows zero operations for all fields when an error happens, hiding what was actually done.

Ensure JSON output displays operation counts consistently with YAML output behavior, correctly showing created, updated, and deleted counts even in error scenarios.

Add support for dropped operations in the summary output, displaying when operations are dropped due to errors or other conditions.

Include unit tests for JSON output formatting and summary generation.
