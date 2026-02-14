# boxlite-ai/boxlite-226

boxlite-ai/boxlite (#226): Refactor: Extract FFI code to dedicated crate

Refactor the SDK so the core FFI logic is housed in a dedicated shared crate that can be reused by multiple language bindings (C, Go, Node.js, Python). Update the C SDK to use this shared crate. The end result should reduce duplication and simplify maintenance without changing user-visible behavior.
