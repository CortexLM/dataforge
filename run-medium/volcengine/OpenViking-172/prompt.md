# volcengine/OpenViking-172

volcengine/OpenViking (#172): feat: make -o and --json global param

Make output format flags global. Ensure the CLI accepts `--json` and `-o json` in any position (before or after subcommands/paths) and applies them consistently to commands like `ls`.
