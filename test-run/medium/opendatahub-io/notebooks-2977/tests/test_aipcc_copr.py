import os
import pathlib
import subprocess

import pytest


SCRIPT_PATH = pathlib.Path("base-images/utils/aipcc.sh")


def _run_bash(script: str, env: dict[str, str]) -> subprocess.CompletedProcess:
    return subprocess.run(
        ["bash", "-c", script],
        env=env,
        text=True,
        capture_output=True,
    )


def _base_env(tmp_path: pathlib.Path) -> dict[str, str]:
    env = os.environ.copy()
    env.update(
        {
            "TARGETARCH": "amd64",
            "PYTHON": "python3",
            "VIRTUAL_ENV": str(tmp_path / "venv"),
        }
    )
    return env


@pytest.mark.parametrize("vendor,expected_calls", [("centos", True), ("rhel", False)])
def test_install_uninstall_copr_calls_dnf(tmp_path, vendor, expected_calls):
    dnf_log = tmp_path / "dnf.log"
    mock_bin = tmp_path / "bin"
    mock_bin.mkdir()
    dnf_script = mock_bin / "dnf"
    dnf_script.write_text(
        "#!/usr/bin/env bash\n"
        "echo \"$@\" >> \"$DNF_LOG\"\n"
    )
    dnf_script.chmod(0o755)

    env = _base_env(tmp_path)
    env["PATH"] = f"{mock_bin}:{env['PATH']}"
    env["DNF_LOG"] = str(dnf_log)

    script = f"""
        set -e
        source {SCRIPT_PATH}
        get_os_vendor() {{ echo {vendor}; }}
        install_copr
        uninstall_copr
    """
    result = _run_bash(script, env)

    assert result.returncode == 0, result.stderr + result.stdout

    log_content = dnf_log.read_text() if dnf_log.exists() else ""
    if expected_calls:
        assert "dnf-command(copr)" in log_content
        assert "copr enable -y aaiet-notebooks/rhelai-el9" in log_content
        assert "copr disable -y aaiet-notebooks/rhelai-el9" in log_content
    else:
        assert log_content.strip() == ""


def test_main_fails_when_hdf5_missing_on_centos(tmp_path):
    lib64 = pathlib.Path("/usr/lib64")
    lib64.mkdir(exist_ok=True)
    libzmq = lib64 / "libzmq.so.5"
    libzmq.touch()

    mock_bin = tmp_path / "bin"
    mock_bin.mkdir()
    dnf_script = mock_bin / "dnf"
    dnf_script.write_text("#!/usr/bin/env bash\nexit 0\n")
    dnf_script.chmod(0o755)

    env = _base_env(tmp_path)
    env["PATH"] = f"{mock_bin}:{env['PATH']}"

    script = f"""
        set -e
        source {SCRIPT_PATH}
        get_os_vendor() {{ echo centos; }}
        install_csb() {{ :; }}
        install_epel() {{ :; }}
        uninstall_epel() {{ :; }}
        install_packages() {{ :; }}
        install_scl_packages() {{ :; }}
        install_python_venv() {{ :; }}
        main
    """
    result = _run_bash(script, env)
    libzmq.unlink(missing_ok=True)

    assert result.returncode == 1
    assert "libhdf5.so.310" in result.stdout + result.stderr
