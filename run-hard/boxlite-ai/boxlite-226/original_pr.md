# boxlite-ai/boxlite-226 (original PR)

boxlite-ai/boxlite (#226): Refactor: Extract FFI code to dedicated crate

Close #225 
This PR extracts the FFI logic from the C SDK into a dedicated `boxlite-ffi` crate.

### Changes
- Moved core FFI operations from `sdks/c/src/ffi.rs` to `sdks/boxlite-ffi/src/ops.rs`.
- Created shared utility modules `error.rs`, `json.rs`, and `string.rs` in `boxlite-ffi`.
- Updated `sdks/c` to depend on the new shared crate.

### Motivation
This refactoring allows reusing the core FFI implementation across multiple language bindings (C, Go, Node.js, Python), significantly reducing code duplication and simplifying maintenance. It is a necessary step for cleaner SDK architecture.

This work also prepares the codebase for the Go SDK implementation. As noted in #202, the Go SDK development will be completed after this PR is merged.
