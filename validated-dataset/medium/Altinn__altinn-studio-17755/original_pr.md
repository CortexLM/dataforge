# Altinn/altinn-studio-17755 (original PR)

Altinn/altinn-studio (#17755): refactor: Move PublishedElements class to the pure-functions library

## Description
This pull request moves the `PublishedElements` class from `content-library` to `pure-functions`. This is necessary because we need these utilities in the layout editor, and not only internally in the content library. Both the exposed funcions of the `PublishedElements` class are pure, satisfying the constraints of the `pure-functions` library.

## Verification
- [x] Related issues are connected (if applicable)
- [x] **Your** code builds clean without any errors or warnings
- [x] Manual testing done (required)
- Adding automated tests not necessary; refactoring only


<!-- This is an auto-generated comment: release notes by coderabbit.ai -->

## Summary by CodeRabbit

* **Refactor**
  * Reorganised internal module imports and exposed PublishedElements functionality through updated package exports. No changes to user-facing functionality or behaviour.

<!-- end of auto-generated comment: release notes by coderabbit.ai -->
