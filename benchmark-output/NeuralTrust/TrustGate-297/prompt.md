# NeuralTrust/TrustGate-297

Add a PostgreSQL-based locking mechanism to prevent concurrent database migrations. When multiple application instances start simultaneously, only one should be allowed to execute migrations while others wait or skip. The lock must be properly released after migrations complete, regardless of success or failure. Ensure migrations are safe to run in horizontally-scaled deployment environments without causing race conditions or conflicts.
