# PostHog/posthog-48030

PostHog/posthog (#48030): fix(phai): replace DataTableNode with DataVisualizationNode

Ensure query/visualization components no longer rely on a data table schema that loads full rows and lacks pagination. Update the system to use a paginated data visualization schema wherever data table nodes are currently expected, so large result sets don’t degrade performance. Behavior should remain the same to users aside from improved handling of large datasets.
