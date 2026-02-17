# opendatahub-io/notebooks-2977

opendatahub-io/notebooks (#2977): ISSUE #2944: chore: (base-images/aipcc.sh): enable aaiet-notebooks/rhelai-el9 COPR repo

Enable a temporary COPR repository that provides newer rebuilds of packages during base image package installation so that dependencies like HDF5 meet required versions. Ensure this repo is enabled during package installation and then disabled/removed before image completion, similar to how EPEL is handled. Add a validation step after package installation to fail with a clear error if the expected newer HDF5 shared library (libhdf5.so.310) is not present, preventing runtime import errors.
