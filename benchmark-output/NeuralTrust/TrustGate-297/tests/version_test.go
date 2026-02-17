package version

import (
	"testing"

	"github.com/stretchr/testify/assert"
)

// TestVersionUpdate verifies the version update in the PR
// This is a fail_to_pass test:
// - On base commit (1.13.0): Test FAILS because version is not 1.13.1
// - On patched commit (1.13.1): Test PASSES because version is 1.13.1
func TestVersionUpdate(t *testing.T) {
	t.Run("version should be 1.13.1", func(t *testing.T) {
		// The PR updates version from 1.13.0 to 1.13.1
		assert.Equal(t, "1.13.1", Version, "Version should be updated to 1.13.1")
	})
}

func TestGetInfo(t *testing.T) {
	t.Run("GetInfo returns correct version", func(t *testing.T) {
		info := GetInfo()
		assert.Equal(t, "TrustGate", info.AppName)
		assert.Equal(t, Version, info.Version)
		assert.NotEmpty(t, info.GoVersion)
	})
}
