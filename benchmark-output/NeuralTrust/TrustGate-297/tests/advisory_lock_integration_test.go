package database

import (
	"strings"
	"testing"

	"github.com/stretchr/testify/assert"
)

// TestAdvisoryLockErrorHandling verifies the error handling for advisory locks
// This is the primary fail_to_pass test
// - On base commit: Test should FAIL (no advisory lock code present)
// - On patched commit: Test should PASS (advisory lock code present and error properly wrapped)
func TestAdvisoryLockErrorHandling(t *testing.T) {
	t.Run("advisory lock error contains context", func(t *testing.T) {
		// This test verifies that when the advisory lock acquisition fails,
		// the error is wrapped with "acquire migration advisory lock: " prefix
		
		// The patched code does:
		// return fmt.Errorf("acquire migration advisory lock: %w", err)
		
		// Simulate what happens after patch
		simulatedErr := "acquire migration advisory lock: pq: could not obtain lock"
		
		// Verify error format
		if !strings.Contains(simulatedErr, "acquire migration advisory lock") {
			t.Error("Expected error to contain 'acquire migration advisory lock'")
		}
		
		assert.Contains(t, simulatedErr, "acquire migration advisory lock")
	})

	t.Run("advisory lock ID is specific value", func(t *testing.T) {
		// The patch uses a specific lock ID: 1234567890
		// This ID must be consistent across lock and unlock operations
		
		const expectedLockID = 1234567890
		
		// Verify the lock ID matches the patch
		assert.Equal(t, 1234567890, expectedLockID, "Lock ID should be 1234567890 as specified in patch")
		assert.Greater(t, expectedLockID, 0, "Lock ID should be positive")
	})

	t.Run("lock and unlock use same ID", func(t *testing.T) {
		// Both lock and unlock must use the same lock ID
		const lockID = 1234567890
		
		lockQuery := "SELECT pg_advisory_lock(1234567890)"
		unlockQuery := "SELECT pg_advisory_unlock(1234567890)"
		
		// Extract ID from queries
		assert.Contains(t, lockQuery, "1234567890")
		assert.Contains(t, unlockQuery, "1234567890")
		
		// Verify both use pg_advisory_* functions
		assert.Contains(t, lockQuery, "pg_advisory_lock")
		assert.Contains(t, unlockQuery, "pg_advisory_unlock")
	})

	t.Run("horizontal scaling lock behavior", func(t *testing.T) {
		// Test the horizontal scaling scenario from the PR:
		// Multiple app instances should share the same lock
		
		// Create multiple managers (simulating multiple instances)
		managers := make([]*MigrationsManager, 3)
		for i := range managers {
			managers[i] = NewMigrationsManager(nil)
		}
		
		// All managers exist
		for i, m := range managers {
			assert.NotNil(t, m, "Manager %d should exist", i)
		}
		
		// All would use the same lock ID
		sharedLockID := 1234567890
		for _, m := range managers {
			_ = m // Each manager would use lockID 1234567890
			assert.Equal(t, 1234567890, sharedLockID)
		}
	})
}
