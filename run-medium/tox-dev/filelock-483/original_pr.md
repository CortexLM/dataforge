# tox-dev/filelock-483 (original PR)

tox-dev/filelock (#483): ✨ feat(mode): respect POSIX default ACL inheritance

When directories have default ACLs configured via `setfacl -d`, newly created files should inherit those permissions.
However, filelock's hardcoded `mode=0o644` passed to `os.open()` combined with an unconditional `os.fchmod()` call
were preventing this — the explicit mode overrides the OS permission machinery, stripping default ACL entries
entirely. This is a significant pain point in multi-user shared environments (e.g. HuggingFace `.cache` directories)
where one user's lock file becomes inaccessible to others, requiring manual deletion. 🔐

The fix introduces a sentinel default for the `mode` parameter. When no explicit mode is passed, lock files are now
created with `0o666` (the standard "let the OS decide" value) and `fchmod()` is skipped, allowing both umask and
default ACLs to control the final permissions naturally. When `mode` is explicitly set by the caller, the existing
behavior is fully preserved — the mode is passed to `os.open()` and enforced via `fchmod()`. ✨ This distinction is
tracked internally via a `_UNSET_FILE_MODE` sentinel so the `.mode` property still returns `0o644` for backward
compatibility.

For users without ACLs and a standard umask (`0o022`), file permissions are identical to before: `0o666 & ~0o022 =
0o644`. The only observable change is for users who have default ACLs configured — which is exactly the population
that needs this fix. Users in security-sensitive environments who want deterministic permissions can explicitly pass
`mode=0o644` to opt into the previous behavior. This approach avoids the thread-safety concerns from #204 since we
never manipulate `umask` — we simply stop overriding the kernel's decision.

Closes #378

See also: https://man7.org/linux/man-pages/man5/acl.5.html (POSIX ACL semantics — "OBJECT CREATION AND DEFAULT ACLs"
section covers how `open()` mode interacts with inherited ACL entries)
