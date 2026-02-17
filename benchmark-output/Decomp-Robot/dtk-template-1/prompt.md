# Decomp-Robot/dtk-template-1

Implement a TOML-based configuration system to replace the existing hardcoded configuration approach. The system must support:

- Defining object states: Matching, NonMatching, and Equivalent
- Version-specific library definitions and compiler flag overrides
- Per-object configuration options including compiler flags (cflags, asflags) and compiler versions
- Hierarchical configuration with default settings that can be overridden by version-specific TOML files
- Library definitions that can vary by project version

Require Python 3.11+ to leverage the tomllib standard library module for TOML parsing. Include comprehensive documentation explaining the configuration file structure, supported options, and how the hierarchical merging of configuration files works.
