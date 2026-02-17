# Decomp-Robot/dtk-template-1 (original PR)

Decomp-Robot/dtk-template (#1): feat: add TOML-based configuration system

Replace configure.py hardcoded config with TOML-based configuration:
- Add tools/config_models.py with dataclasses for config structure
- Add tools/config_loader.py for loading and merging TOML files
- Add config/default.toml with tool versions and build flags
- Add config/libs.toml with default library definitions
- Add config/{VERSION}/libs.toml for version-specific libraries
- Add config/{VERSION}/flags.toml for version-specific flag overrides
- Support Matching/NonMatching/Equivalent object states
- Support version-specific objects via versions field
- Support per-object options (cflags, asflags, mw_version, etc.)
- Add documentation in docs/configuration.md

Requires Python 3.11+ for tomllib stdlib.
