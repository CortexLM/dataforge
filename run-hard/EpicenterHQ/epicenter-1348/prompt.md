# EpicenterHQ/epicenter-1348

EpicenterHQ/epicenter (#1348): refactor: extract @epicenter/server into standalone package

Extract the sync server into its own installable package so users can self-host without installing the full workspace system. Ensure the server can be used independently from the core library and that the CLI’s serve command continues to work without introducing circular dependencies. Expose any required public types so external consumers can use the server. Update documentation accordingly.
