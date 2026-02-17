import os
import pathlib
import subprocess
import tempfile
import unittest


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


class TestAipccCopr(unittest.TestCase):
    def test_install_uninstall_copr_calls_dnf_for_centos(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            tmp_path = pathlib.Path(tmpdir)
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
                get_os_vendor() {{ echo centos; }}
                install_copr
                uninstall_copr
            """
            result = _run_bash(script, env)

            self.assertEqual(result.returncode, 0, result.stderr + result.stdout)

            log_content = dnf_log.read_text() if dnf_log.exists() else ""
            self.assertIn("dnf-command(copr)", log_content)
            self.assertIn("copr enable -y aaiet-notebooks/rhelai-el9", log_content)
            self.assertIn("copr disable -y aaiet-notebooks/rhelai-el9", log_content)

    def test_install_uninstall_copr_skips_non_centos(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            tmp_path = pathlib.Path(tmpdir)
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
                get_os_vendor() {{ echo rhel; }}
                install_copr
                uninstall_copr
            """
            result = _run_bash(script, env)

            self.assertEqual(result.returncode, 0, result.stderr + result.stdout)

            log_content = dnf_log.read_text() if dnf_log.exists() else ""
            self.assertEqual(log_content.strip(), "")

    def test_main_fails_when_hdf5_missing_on_centos(self):
        lib64 = pathlib.Path("/usr/lib64")
        lib64.mkdir(exist_ok=True)
        libzmq = lib64 / "libzmq.so.5"
        libzmq.touch()

        with tempfile.TemporaryDirectory() as tmpdir:
            tmp_path = pathlib.Path(tmpdir)
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

        self.assertEqual(result.returncode, 1)
        self.assertIn("libhdf5.so.310", result.stdout + result.stderr)
