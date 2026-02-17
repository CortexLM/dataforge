# run-house/kubetorch-2243

Add configuration options to specify custom remote directory paths and import paths for modules. When running modules on remote clusters, users should be able to:

1. Specify a custom `remote_dir` to control where module code is placed on the remote filesystem
2. Specify a `remote_import_path` to control how the module is added to the Python path for imports on the remote side

These options should allow users to override default behaviors and have full control over module placement and import resolution when code is executed remotely.
