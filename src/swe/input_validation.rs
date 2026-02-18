//! Shared input validation functions for shell-safe interpolation.
//!
//! These validators ensure that user-controlled strings (repository names,
//! git refs, container paths) are safe to interpolate into shell commands
//! executed inside Docker containers.

use anyhow::Result;

/// Validate a GitHub repository name (`owner/repo`).
///
/// Rejects values that do not match the `owner/repo` format or contain
/// characters outside the set allowed by GitHub: alphanumeric, `-`, `_`, `.`.
pub fn validate_repo_name(repo: &str) -> Result<()> {
    if repo.is_empty() {
        anyhow::bail!("repository name must not be empty");
    }
    let parts: Vec<&str> = repo.splitn(2, '/').collect();
    if parts.len() != 2 || parts[0].is_empty() || parts[1].is_empty() {
        anyhow::bail!(
            "repository name must be in 'owner/repo' format, got '{}'",
            repo
        );
    }
    for ch in repo.chars() {
        if !ch.is_alphanumeric() && ch != '/' && ch != '-' && ch != '_' && ch != '.' {
            anyhow::bail!(
                "repository name contains invalid character '{}': '{}'",
                ch,
                repo
            );
        }
    }
    Ok(())
}

/// Validate a git ref (commit SHA, branch name, or ref range like `a..b`).
///
/// Only allows alphanumeric chars and the limited set `-`, `_`, `.`, `/`,
/// `~`, `^` which are valid in git ref specifications.
pub fn validate_git_ref(git_ref: &str) -> Result<()> {
    if git_ref.is_empty() {
        anyhow::bail!("git ref must not be empty");
    }
    for ch in git_ref.chars() {
        if !ch.is_alphanumeric() && !"-_.~/^".contains(ch) {
            anyhow::bail!("git ref contains invalid character '{}': '{}'", ch, git_ref);
        }
    }
    Ok(())
}

/// Validate a file path for use inside a container.
///
/// Uses an **allowlist** approach: only permits alphanumeric characters and
/// the path-safe set `/`, `-`, `_`, `.`, `+`, `@`, ` ` (space). Rejects
/// path traversal (`..`), absolute paths, and anything else.
pub fn validate_container_path(path: &str) -> Result<()> {
    if path.is_empty() {
        anyhow::bail!("container path must not be empty");
    }
    if path.starts_with('/') {
        anyhow::bail!("container path must be relative, got '{}'", path);
    }
    if path.contains("..") {
        anyhow::bail!("container path must not contain '..': '{}'", path);
    }
    for ch in path.chars() {
        if ch.is_alphanumeric()
            || ch == '/'
            || ch == '-'
            || ch == '_'
            || ch == '.'
            || ch == '+'
            || ch == '@'
            || ch == ' '
        {
            continue;
        }
        anyhow::bail!(
            "container path contains disallowed character '{}': '{}'",
            ch,
            path
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- validate_repo_name ---

    #[test]
    fn repo_name_valid() {
        assert!(validate_repo_name("owner/repo").is_ok());
        assert!(validate_repo_name("my-org/my_repo.rs").is_ok());
        assert!(validate_repo_name("A/B").is_ok());
    }

    #[test]
    fn repo_name_empty() {
        assert!(validate_repo_name("").is_err());
    }

    #[test]
    fn repo_name_no_slash() {
        assert!(validate_repo_name("noslash").is_err());
    }

    #[test]
    fn repo_name_empty_parts() {
        assert!(validate_repo_name("/repo").is_err());
        assert!(validate_repo_name("owner/").is_err());
    }

    #[test]
    fn repo_name_shell_metachar() {
        assert!(validate_repo_name("owner/repo;rm -rf").is_err());
        assert!(validate_repo_name("owner/repo$(cmd)").is_err());
        assert!(validate_repo_name("owner/repo`id`").is_err());
        assert!(validate_repo_name("owner/repo|cat").is_err());
        assert!(validate_repo_name("owner/repo&bg").is_err());
        assert!(validate_repo_name("owner/repo\ninjected").is_err());
    }

    // --- validate_git_ref ---

    #[test]
    fn git_ref_valid() {
        assert!(validate_git_ref("abc123").is_ok());
        assert!(validate_git_ref("main").is_ok());
        assert!(validate_git_ref("feature/branch-name").is_ok());
        assert!(validate_git_ref("HEAD~1").is_ok());
        assert!(validate_git_ref("v1.2.3").is_ok());
        assert!(validate_git_ref("HEAD^2").is_ok());
    }

    #[test]
    fn git_ref_empty() {
        assert!(validate_git_ref("").is_err());
    }

    #[test]
    fn git_ref_shell_metachar() {
        assert!(validate_git_ref("ref;cmd").is_err());
        assert!(validate_git_ref("ref$(cmd)").is_err());
        assert!(validate_git_ref("ref`id`").is_err());
        assert!(validate_git_ref("ref|cat").is_err());
        assert!(validate_git_ref("ref&bg").is_err());
        assert!(validate_git_ref("ref\nline2").is_err());
        assert!(validate_git_ref("ref'quote").is_err());
    }

    // --- validate_container_path ---

    #[test]
    fn container_path_valid() {
        assert!(validate_container_path("src/main.rs").is_ok());
        assert!(validate_container_path("tests/test_foo.py").is_ok());
        assert!(validate_container_path("dir/sub dir/file.txt").is_ok());
        assert!(validate_container_path("file+extra@v2.txt").is_ok());
        assert!(validate_container_path("a-b_c.d").is_ok());
    }

    #[test]
    fn container_path_empty() {
        assert!(validate_container_path("").is_err());
    }

    #[test]
    fn container_path_absolute() {
        assert!(validate_container_path("/etc/passwd").is_err());
    }

    #[test]
    fn container_path_traversal() {
        assert!(validate_container_path("../etc/passwd").is_err());
        assert!(validate_container_path("foo/../../bar").is_err());
    }

    #[test]
    fn container_path_shell_metachar_blocked() {
        assert!(validate_container_path("file'name").is_err());
        assert!(validate_container_path("file\"name").is_err());
        assert!(validate_container_path("file`cmd`").is_err());
        assert!(validate_container_path("file$var").is_err());
        assert!(validate_container_path("file;cmd").is_err());
        assert!(validate_container_path("file|pipe").is_err());
        assert!(validate_container_path("file&bg").is_err());
        assert!(validate_container_path("file\nline").is_err());
        assert!(validate_container_path("file\rline").is_err());
        assert!(validate_container_path("file\0null").is_err());
    }

    #[test]
    fn container_path_extra_metachar_blocked() {
        assert!(validate_container_path("file(cmd)").is_err());
        assert!(validate_container_path("file{a,b}").is_err());
        assert!(validate_container_path("file>out").is_err());
        assert!(validate_container_path("file<in").is_err());
        assert!(validate_container_path("file!hist").is_err());
        assert!(validate_container_path("file#comment").is_err());
        assert!(validate_container_path("file\\escaped").is_err());
        assert!(validate_container_path("file~home").is_err());
    }
}
