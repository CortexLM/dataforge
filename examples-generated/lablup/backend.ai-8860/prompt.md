# lablup/backend.ai-8860

lablup/backend.ai (#8860): refactor(BA-4402): Create SystemComposer for system services

## Summary
- Add `dependencies/system/` package with 8 `DependencyProvider` implementations covering system services across Layer 0-4: `CORSOptionsDependency`, `MetricsDependency`, `GQLAdapterDependency`, `JWTValidatorDependency`, `PrometheusClientDependency`, `ServiceDiscoveryDependency`, `BackgroundTaskManagerDependency`, `HealthProbeDependency`
- Add `SystemComposer` that orchestrates initialization order across 4 dependency layers with proper cleanup on teardown
- Add unit tests for composer lifecycle and all provider stage names

## Test plan
- [x] All provider `stage_name` properties verified
- [x] `SystemComposer.compose()` lifecycle tested with mock dependencies (init + cleanup)
- [x] `pants fmt/fix/lint/check` pass
- [x] `pants test --changed-dependents=transitive` pass

Resolves BA-4402

🤖 Generated with [Claude Code](https://claude.com/claude-code)
