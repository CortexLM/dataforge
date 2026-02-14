# tox-dev/filelock-483

tox-dev/filelock (#483): ✨ feat(mode): respect POSIX default ACL inheritance

Ensure lock file creation respects POSIX default ACL inheritance. When callers do not specify a file mode, allow the OS (umask and default ACLs) to determine final permissions instead of forcing a fixed mode. Preserve existing behavior when a mode is explicitly provided, so permissions are enforced deterministically in that case. Maintain backward compatibility in exposed mode reporting while changing only the observable behavior for environments with default ACLs.
