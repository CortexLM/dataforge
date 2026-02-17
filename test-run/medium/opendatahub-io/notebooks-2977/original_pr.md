# opendatahub-io/notebooks-2977 (original PR)

opendatahub-io/notebooks (#2977): ISSUE #2944: chore: (base-images/aipcc.sh): enable aaiet-notebooks/rhelai-el9 COPR repo

* https://github.com/opendatahub-io/notebooks/pull/2958

## Context

The file [base-images/utils/aipcc.sh](base-images/utils/aipcc.sh) currently installs EPEL temporarily (lines 360-361, 395) and uninstalls it at the end (lines 364-365, 414). The problem is that some EPEL packages are too old -- e.g., `hdf5` wants version 3.10 but CentOS/EPEL has an older version, causing SO version mismatches with Python wheels from the RH index.

The COPR repository at `https://copr.fedorainfracloud.org/coprs/aaiet-notebooks/rhelai-el9/` contains newer rebuilds of these packages. Enabling it alongside EPEL allows `dnf` to pick up the newer versions during `install_packages`.

## Current flow in `main()` (lines 392-415)

```
install_csb        # enable CRB/crb repo
install_epel       # install EPEL release RPM
dnf update --security
install_packages   # install all RPMs (uses EPEL)
...
uninstall_epel     # remove EPEL at end
```

## Plan

### 1. Add `install_copr` and `uninstall_copr` functions

Add two new functions in [base-images/utils/aipcc.sh](base-images/utils/aipcc.sh), next to the existing `install_epel`/`uninstall_epel` pair (around line 366):

```bash
function install_copr() {
    dnf install "${DNF_OPTS[@]}" 'dnf-command(copr)'
    dnf copr enable -y aaiet-notebooks/rhelai-el9
}

function uninstall_copr() {
    dnf copr disable -y aaiet-notebooks/rhelai-el9
}
```

The `dnf copr enable` command adds a `.repo` file under `/etc/yum.repos.d/` that points to the COPR repo. This makes the newer package versions available for resolution. The `priority` in COPR repos is typically set so that COPR packages override base/EPEL packages of the same name.

### 2. Call `install_copr` after `install_epel` in `main()`

In the `main()` function (line 392), add the COPR enable call right after EPEL is installed, and disable it right before EPEL is uninstalled:

```bash
function main() {
    install_csb

    install_epel
    install_copr

    # install security updates
    dnf update "${DNF_OPTS[@]}" --security

    install_packages
    ...

    uninstall_copr
    uninstall_epel
}
```

### 3. Add `libhdf5.so.310` existence check

Add a check in `main()` right after the existing `libzmq.so.5` check (line 401-404), to verify the COPR repo is actually delivering the newer hdf5 package. This directly validates the fix for [issue #2944](https://github.com/opendatahub-io/notebooks/issues/2944) (`ImportError: libhdf5.so.310: cannot open shared object file`):

```bash
    if ! test -f /usr/lib64/libhdf5.so.310; then
        echo "Error: libhdf5.so.310 was not found after installation (see https://github.com/opendatahub-io/notebooks/issues/2944)"
        exit 1
    fi
```

This mirrors the existing pattern for `libzmq.so.5` on line 401-404 of [base-images/utils/aipcc.sh](base-images/utils/aipcc.sh).

### 4. No c9s/ubi conditional gating

Both c9s and ubi9 variants run the same `aipcc.sh` script. The COPR repo targets `epel-9-*` chroots, which work on both CentOS Stream 9 and UBI 9 (both are RHEL 9 derivatives). EPEL packages are EPEL packages regardless of the base -- both variants already install EPEL, so the COPR overlay applies equally.

The existing `os_vendor` checks in `aipcc.sh` gate specific *packages* that are unavailable on ubi9 (like `snappy`, `openmpi`, `geos`), but the *repo enablement* itself works on both.

## Notes

- The COPR repo is cleaned up at the end (just like EPEL) to avoid leaving third-party repos in the final image.
- The `dnf-command(copr)` virtual provide installs the `dnf-plugins-core` package (or it may already be present since `install_csb` also installs it). This is needed for the `dnf copr` subcommand.
- The `priority` of the COPR repo should cause its packages to override older EPEL versions by default. If not, we may need to add `--setopt=priority=50` or use `cost` settings, but this is unlikely to be necessary.

## How Has This Been Tested?

* https://github.com/opendatahub-io/notebooks/actions/runs/22078006866?pr=2943

This PR used to fail on inability to load Keras, so hopefully I can get past that now.

Self checklist (all need to be checked):
- [ ] Ensure that you have run `make test` (`gmake` on macOS) before asking for review
- [ ] Changes to everything except `Dockerfile.konflux` files should be done in `odh/notebooks` and automatically synced to `rhds/notebooks`. For Konflux-specific changes, modify `Dockerfile.konflux` files directly in `rhds/notebooks` as these require special attention in the downstream repository and flow to the upcoming RHOAI release.

## Merge criteria:
<!--- This PR will be merged by any repository approver when it meets all the points in the checklist -->
<!--- Go over all the following points, and put an `x` in all the boxes that apply. -->

- [ ] The commits are squashed in a cohesive manner and have meaningful messages.
- [ ] Testing instructions have been added in the PR body (for PRs involving changes that are not immediately obvious).
- [ ] The developer has manually tested the changes and verified that the changes work


<!-- This is an auto-generated comment: release notes by coderabbit.ai -->
## Summary by CodeRabbit

* **Chores**
  * Enabled use of additional COPR repositories during builds to access newer package versions; the temporary repo is enabled when needed and removed afterward.
  * Added runtime validation to verify critical libraries (e.g., libzmq, HDF5 on CentOS) after installation and fail early with clear errors if missing.
<!-- end of auto-generated comment: release notes by coderabbit.ai -->
