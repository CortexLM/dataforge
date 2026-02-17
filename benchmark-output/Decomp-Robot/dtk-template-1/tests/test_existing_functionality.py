"""Tests for existing functionality that should continue to work after the PR.

This module tests that existing project functionality is not broken
by the TOML configuration system changes.
"""

import sys
from pathlib import Path


def test_project_config_import():
    """Test that ProjectConfig can still be imported."""
    from tools.project import ProjectConfig, Object, ProgressCategory
    
    config = ProjectConfig()
    assert config.version is None
    assert config.build_dir == Path("build")


def test_object_class():
    """Test that Object class still works."""
    from tools.project import Object
    
    obj = Object(completed=True, name="test.c")
    assert obj.name == "test.c"
    assert obj.completed == True
    assert obj.options["add_to_all"] is None


def test_project_config_attributes():
    """Test that ProjectConfig has expected attributes."""
    from tools.project import ProjectConfig
    
    config = ProjectConfig()
    
    # Check key attributes exist
    assert hasattr(config, 'build_dir')
    assert hasattr(config, 'src_dir')
    assert hasattr(config, 'tools_dir')
    assert hasattr(config, 'binutils_tag')
    assert hasattr(config, 'compilers_tag')
    assert hasattr(config, 'dtk_tag')
    assert hasattr(config, 'asflags')
    assert hasattr(config, 'ldflags')
    assert hasattr(config, 'libs')


def test_is_windows_function():
    """Test that is_windows function works."""
    from tools.project import is_windows
    
    result = is_windows()
    assert isinstance(result, bool)


if __name__ == "__main__":
    print("Running existing functionality tests...")
    
    test_project_config_import()
    print("  PASS: ProjectConfig import")
    
    test_object_class()
    print("  PASS: Object class")
    
    test_project_config_attributes()
    print("  PASS: ProjectConfig attributes")
    
    test_is_windows_function()
    print("  PASS: is_windows function")
    
    print("\n" + "=" * 50)
    print("ALL EXISTING FUNCTIONALITY TESTS PASSED!")
    print("=" * 50)
