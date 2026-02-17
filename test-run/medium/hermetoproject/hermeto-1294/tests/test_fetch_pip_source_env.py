import pytest

from hermeto.core.models.input import Request
from hermeto.core.models.output import EnvironmentVariable
from hermeto.core.package_managers.pip import main as pip
from hermeto.core.rooted_path import RootedPath


def test_fetch_pip_source_sets_env_vars_without_pip_packages(tmp_path):
    source_dir = RootedPath(tmp_path)
    output_dir = RootedPath(tmp_path / "output")
    app_dir = tmp_path / "app"
    app_dir.mkdir()

    request = Request(
        source_dir=source_dir,
        output_dir=output_dir,
        packages=[{"type": "npm", "path": "app"}],
    )

    output = pip.fetch_pip_source(request)

    assert output.build_config.environment_variables == [
        EnvironmentVariable(name="PIP_FIND_LINKS", value="${output_dir}/deps/pip"),
        EnvironmentVariable(name="PIP_NO_INDEX", value="true"),
    ]
    assert output.components == []
