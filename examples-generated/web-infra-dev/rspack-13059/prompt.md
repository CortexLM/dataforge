# web-infra-dev/rspack-13059

web-infra-dev/rspack (#13059): refactor: Bind ImportedByDeferModulesArtifact to optimize chunk modules pass

Summary
- document that artifacts must declare the pass where they are first created and update `ImportedByDeferModulesArtifact` to use the new `OPTIMIZE_CHUNK_MODULES` pass
- add the new pass bit, expose it through the JS/Rust incremental options, and default it on so the pass participates in the incremental flow
- hook the cache into `before/after_optimize_chunk_modules`, recover the artifact there, and wire the pass into the optimize chunk modules implementation

Testing
- Not run (not requested)
