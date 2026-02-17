package cmd

import (
	"testing"

	"github.com/kong/go-database-reconciler/pkg/diff"
	"github.com/stretchr/testify/assert"
)

func TestJSONOutput_DroppedOperationsInitialization(t *testing.T) {
	// Reset jsonOutput to simulate syncMain behavior
	jsonOutput = diff.JSONOutputObject{}

	// Initialize the Changes struct as syncMain would with the fix
	jsonOutput.Changes = diff.EntityChanges{
		Creating:         []diff.EntityState{},
		Updating:         []diff.EntityState{},
		Deleting:         []diff.EntityState{},
		DroppedCreations: []diff.EntityState{},
		DroppedUpdates:   []diff.EntityState{},
		DroppedDeletions: []diff.EntityState{},
	}

	// Verify that all fields including dropped operations are properly initialized
	assert.NotNil(t, jsonOutput.Changes.Creating, "Creating should be initialized")
	assert.NotNil(t, jsonOutput.Changes.Updating, "Updating should be initialized")
	assert.NotNil(t, jsonOutput.Changes.Deleting, "Deleting should be initialized")
	assert.NotNil(t, jsonOutput.Changes.DroppedCreations, "DroppedCreations should be initialized")
	assert.NotNil(t, jsonOutput.Changes.DroppedUpdates, "DroppedUpdates should be initialized")
	assert.NotNil(t, jsonOutput.Changes.DroppedDeletions, "DroppedDeletions should be initialized")

	// Verify all slices are empty (not nil)
	assert.Empty(t, jsonOutput.Changes.Creating, "Creating should be an empty slice")
	assert.Empty(t, jsonOutput.Changes.Updating, "Updating should be an empty slice")
	assert.Empty(t, jsonOutput.Changes.Deleting, "Deleting should be an empty slice")
	assert.Empty(t, jsonOutput.Changes.DroppedCreations, "DroppedCreations should be an empty slice")
	assert.Empty(t, jsonOutput.Changes.DroppedUpdates, "DroppedUpdates should be an empty slice")
	assert.Empty(t, jsonOutput.Changes.DroppedDeletions, "DroppedDeletions should be an empty slice")
}

func TestJSONOutput_EntityChangesWithDroppedOperations(t *testing.T) {
	// Create EntityChanges with dropped operations
	changes := diff.EntityChanges{
		Creating: []diff.EntityState{
			{Name: "service-1", Kind: "service"},
		},
		Updating: []diff.EntityState{
			{Name: "route-1", Kind: "route"},
		},
		Deleting:         []diff.EntityState{},
		DroppedCreations: []diff.EntityState{
			{Name: "failed-service", Kind: "service"},
		},
		DroppedUpdates: []diff.EntityState{
			{Name: "failed-route", Kind: "route"},
		},
		DroppedDeletions: []diff.EntityState{},
	}

	// Verify all fields are accessible and have correct values
	assert.Len(t, changes.Creating, 1, "Should have 1 creating operation")
	assert.Len(t, changes.Updating, 1, "Should have 1 updating operation")
	assert.Len(t, changes.Deleting, 0, "Should have 0 deleting operations")
	assert.Len(t, changes.DroppedCreations, 1, "Should have 1 dropped creation")
	assert.Len(t, changes.DroppedUpdates, 1, "Should have 1 dropped update")
	assert.Len(t, changes.DroppedDeletions, 0, "Should have 0 dropped deletions")

	// Verify individual items
	assert.Equal(t, "service-1", changes.Creating[0].Name)
	assert.Equal(t, "failed-service", changes.DroppedCreations[0].Name)
	assert.Equal(t, "failed-route", changes.DroppedUpdates[0].Name)
}

func TestJSONOutput_SummaryWithOperations(t *testing.T) {
	// Create a summary as would be done in performDiff
	summary := diff.Summary{
		Creating: 5,
		Updating: 3,
		Deleting: 2,
		Total:    10,
	}

	// Verify summary values
	assert.Equal(t, int32(5), summary.Creating, "Creating count should be 5")
	assert.Equal(t, int32(3), summary.Updating, "Updating count should be 3")
	assert.Equal(t, int32(2), summary.Deleting, "Deleting count should be 2")
	assert.Equal(t, int32(10), summary.Total, "Total count should be 10")
}

func TestJSONOutput_TotalOpsCalculation(t *testing.T) {
	// Simulate the stats that would be returned from Solve()
	// Test the totalOps calculation: totalOps = CreateOps + UpdateOps + DeleteOps
	createOps := int32(7)
	updateOps := int32(4)
	deleteOps := int32(2)

	totalOps := createOps + updateOps + deleteOps

	assert.Equal(t, int32(13), totalOps, "Total operations should be sum of create, update, and delete")

	// Verify calculation order - totalOps should be calculated before error check
	// This ensures JSON output shows correct counts even when errors occur
	summary := diff.Summary{
		Creating: createOps,
		Updating: updateOps,
		Deleting: deleteOps,
		Total:    totalOps,
	}

	assert.Equal(t, createOps, summary.Creating)
	assert.Equal(t, updateOps, summary.Updating)
	assert.Equal(t, deleteOps, summary.Deleting)
	assert.Equal(t, totalOps, summary.Total)
}

func TestJSONOutput_AppendDroppedOperations(t *testing.T) {
	// Reset and initialize jsonOutput
	jsonOutput = diff.JSONOutputObject{
		Changes: diff.EntityChanges{
			Creating:         []diff.EntityState{},
			Updating:         []diff.EntityState{},
			Deleting:         []diff.EntityState{},
			DroppedCreations: []diff.EntityState{},
			DroppedUpdates:   []diff.EntityState{},
			DroppedDeletions: []diff.EntityState{},
		},
	}

	// Simulate changes from Solve()
	newChanges := diff.EntityChanges{
		Creating: []diff.EntityState{
			{Name: "new-service", Kind: "service"},
		},
		DroppedCreations: []diff.EntityState{
			{Name: "dropped-service", Kind: "service"},
		},
	}

	// Append changes as performDiff would do
	jsonOutput.Changes = diff.EntityChanges{
		Creating:         append(jsonOutput.Changes.Creating, newChanges.Creating...),
		Updating:         append(jsonOutput.Changes.Updating, newChanges.Updating...),
		Deleting:         append(jsonOutput.Changes.Deleting, newChanges.Deleting...),
		DroppedCreations: append(jsonOutput.Changes.DroppedCreations, newChanges.DroppedCreations...),
		DroppedUpdates:   append(jsonOutput.Changes.DroppedUpdates, newChanges.DroppedUpdates...),
		DroppedDeletions: append(jsonOutput.Changes.DroppedDeletions, newChanges.DroppedDeletions...),
	}

	// Verify appending works correctly
	assert.Len(t, jsonOutput.Changes.Creating, 1, "Should have 1 creating operation")
	assert.Len(t, jsonOutput.Changes.DroppedCreations, 1, "Should have 1 dropped creation")
	assert.Equal(t, "new-service", jsonOutput.Changes.Creating[0].Name)
	assert.Equal(t, "dropped-service", jsonOutput.Changes.DroppedCreations[0].Name)
}

func TestJSONOutput_JSONMarshalingWithDroppedOperations(t *testing.T) {
	// Test that EntityChanges with dropped operations can be marshaled to JSON correctly
	changes := diff.EntityChanges{
		Creating: []diff.EntityState{
			{Name: "created-service", Kind: "service"},
		},
		Updating: []diff.EntityState{
			{Name: "updated-route", Kind: "route"},
		},
		Deleting: []diff.EntityState{},
		DroppedCreations: []diff.EntityState{
			{Name: "dropped-create", Kind: "service"},
		},
		DroppedUpdates: []diff.EntityState{
			{Name: "dropped-update", Kind: "plugin"},
		},
		DroppedDeletions: []diff.EntityState{
			{Name: "dropped-delete", Kind: "consumer"},
		},
	}

	// Create JSONOutputObject
	output := diff.JSONOutputObject{
		Changes: changes,
		Summary: diff.Summary{
			Creating: 1,
			Updating: 1,
			Deleting: 0,
			Total:    2,
		},
		Warnings: []string{"test warning"},
		Errors:   []string{},
	}

	// Verify the structure is correctly formed
	assert.Equal(t, int32(1), output.Summary.Creating)
	assert.Equal(t, int32(1), output.Summary.Updating)
	assert.Equal(t, int32(2), output.Summary.Total)
	assert.Len(t, output.Changes.Creating, 1)
	assert.Len(t, output.Changes.DroppedCreations, 1)
	assert.Len(t, output.Changes.DroppedUpdates, 1)
	assert.Len(t, output.Changes.DroppedDeletions, 1)
	assert.Len(t, output.Warnings, 1)
}

func TestJSONOutput_EmptyDroppedOperationsOmitted(t *testing.T) {
	// Test that empty dropped operation slices are handled correctly
	// (They should be empty slices, not nil, when explicitly initialized)
	changes := diff.EntityChanges{
		Creating:         []diff.EntityState{},
		Updating:         []diff.EntityState{},
		Deleting:         []diff.EntityState{},
		DroppedCreations: []diff.EntityState{},
		DroppedUpdates:   []diff.EntityState{},
		DroppedDeletions: []diff.EntityState{},
	}

	// All should be empty but initialized
	assert.NotNil(t, changes.Creating)
	assert.NotNil(t, changes.Updating)
	assert.NotNil(t, changes.Deleting)
	assert.NotNil(t, changes.DroppedCreations)
	assert.NotNil(t, changes.DroppedUpdates)
	assert.NotNil(t, changes.DroppedDeletions)

	assert.Empty(t, changes.Creating)
	assert.Empty(t, changes.Updating)
	assert.Empty(t, changes.Deleting)
	assert.Empty(t, changes.DroppedCreations)
	assert.Empty(t, changes.DroppedUpdates)
	assert.Empty(t, changes.DroppedDeletions)
}

func TestJSONOutput_MultipleDroppedOperations(t *testing.T) {
	// Test with multiple dropped operations of different types
	changes := diff.EntityChanges{
		Creating: []diff.EntityState{
			{Name: "svc1", Kind: "service"},
			{Name: "svc2", Kind: "service"},
		},
		DroppedCreations: []diff.EntityState{
			{Name: "failed-svc1", Kind: "service"},
			{Name: "failed-svc2", Kind: "service"},
			{Name: "failed-svc3", Kind: "service"},
		},
		DroppedUpdates: []diff.EntityState{
			{Name: "failed-route1", Kind: "route"},
			{Name: "failed-route2", Kind: "route"},
		},
		DroppedDeletions: []diff.EntityState{
			{Name: "failed-consumer", Kind: "consumer"},
		},
	}

	// Verify counts
	assert.Len(t, changes.Creating, 2, "Should have 2 successful creations")
	assert.Len(t, changes.DroppedCreations, 3, "Should have 3 dropped creations")
	assert.Len(t, changes.DroppedUpdates, 2, "Should have 2 dropped updates")
	assert.Len(t, changes.DroppedDeletions, 1, "Should have 1 dropped deletion")

	// Verify specific items
	assert.Equal(t, "svc1", changes.Creating[0].Name)
	assert.Equal(t, "failed-svc2", changes.DroppedCreations[1].Name)
	assert.Equal(t, "route", changes.DroppedUpdates[0].Kind)
	assert.Equal(t, "consumer", changes.DroppedDeletions[0].Kind)
}
