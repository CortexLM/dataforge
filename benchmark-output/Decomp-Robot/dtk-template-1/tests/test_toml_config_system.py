"""Tests for TOML-based configuration system modules.

This module tests the new TOML configuration system that replaces
the existing hardcoded configuration approach.
"""

import sys
import tempfile
from pathlib import Path


def test_config_models_import():
    """Test that config_models module can be imported."""
    from tools.config_models import ToolVersions, BuildFlags, ObjectDef, LibraryDef


def test_config_loader_import():
    """Test that config_loader module can be imported."""
    from tools.config_loader import ConfigLoader, MergedConfig, load_config


def test_tool_versions_dataclass():
    """Test ToolVersions dataclass with default values."""
    from tools.config_models import ToolVersions
    
    tools = ToolVersions()
    assert tools.binutils_tag == "2.42-1"
    assert tools.compilers_tag == "20251118"
    assert tools.dtk_tag == "v1.8.0"
    assert tools.wibo_tag is None


def test_build_flags_dataclass():
    """Test BuildFlags dataclass with default values."""
    from tools.config_models import BuildFlags
    
    flags = BuildFlags()
    assert flags.linker_version == "GC/1.2.5n"
    assert isinstance(flags.cflags_base, list)


def test_object_def_dataclass():
    """Test ObjectDef dataclass."""
    from tools.config_models import ObjectDef
    
    obj = ObjectDef(name="test.c")
    assert obj.name == "test.c"
    assert obj.completed == False
    assert obj.equivalent == False


def test_library_def_dataclass():
    """Test LibraryDef dataclass."""
    from tools.config_models import LibraryDef, ObjectDef
    
    lib = LibraryDef(name="Game", mw_version="GC/1.3.2")
    assert lib.name == "Game"
    assert lib.mw_version == "GC/1.3.2"


def test_config_loader_initialization():
    """Test ConfigLoader initialization."""
    from tools.config_loader import ConfigLoader
    
    with tempfile.TemporaryDirectory() as tmpdir:
        config_path = Path(tmpdir)
        loader = ConfigLoader(config_path)
        assert loader.config_dir == config_path


def test_config_loader_load_toml():
    """Test ConfigLoader.load_toml method."""
    from tools.config_loader import ConfigLoader
    
    with tempfile.TemporaryDirectory() as tmpdir:
        config_path = Path(tmpdir)
        loader = ConfigLoader(config_path)
        
        # Test loading non-existent file
        result = loader.load_toml(config_path / "nonexistent.toml")
        assert result is None
        
        # Test loading existing file
        toml_file = config_path / "test.toml"
        toml_file.write_bytes(b"""
[project]
name = "Test"
""")
        result = loader.load_toml(toml_file)
        assert result is not None
        assert result["project"]["name"] == "Test"


def test_config_loader_parse_tool_versions():
    """Test parsing tool versions from TOML data."""
    from tools.config_loader import ConfigLoader
    from tools.config_models import ToolVersions
    
    with tempfile.TemporaryDirectory() as tmpdir:
        loader = ConfigLoader(Path(tmpdir))
        
        data = {
            "tools": {
                "binutils_tag": "2.40",
                "dtk_tag": "v1.9.0",
            }
        }
        result = loader.parse_tool_versions(data)
        assert result.binutils_tag == "2.40"
        assert result.dtk_tag == "v1.9.0"


def test_config_loader_parse_build_flags():
    """Test parsing build flags from TOML data."""
    from tools.config_loader import ConfigLoader
    
    with tempfile.TemporaryDirectory() as tmpdir:
        loader = ConfigLoader(Path(tmpdir))
        
        data = {
            "build": {
                "linker_version": "GC/1.3.2",
                "asflags": ["-mgekko"],
            }
        }
        result = loader.parse_build_flags(data)
        assert result.linker_version == "GC/1.3.2"
        assert "-mgekko" in result.asflags


def test_config_loader_parse_libraries():
    """Test parsing library definitions from TOML data."""
    from tools.config_loader import ConfigLoader
    
    with tempfile.TemporaryDirectory() as tmpdir:
        loader = ConfigLoader(Path(tmpdir))
        
        data = {
            "lib": [{
                "name": "Game",
                "mw_version": "GC/1.3.2",
                "object": [
                    {"name": "main.c", "completed": False},
                ]
            }]
        }
        result = loader.parse_libraries(data)
        assert len(result) == 1
        assert result[0].name == "Game"


def test_load_config_integration():
    """Test the full load_config integration."""
    from tools.config_loader import load_config
    
    with tempfile.TemporaryDirectory() as tmpdir:
        config_dir = Path(tmpdir) / "config"
        config_dir.mkdir()
        
        # Create default.toml
        default_toml = config_dir / "default.toml"
        default_toml.write_bytes(b"""
[project]
default_version = "GAMEID"

[tools]
binutils_tag = "2.42-1"
dtk_tag = "v1.8.0"

[build]
linker_version = "GC/1.3.2"
asflags = ["-mgekko"]

[progress.categories]
game = "Game"
""")
        
        # Create libs.toml
        libs_toml = config_dir / "libs.toml"
        libs_toml.write_bytes(b"""
[[lib]]
name = "Runtime"
mw_version = "GC/1.2.5"

[[lib.object]]
name = "runtime.c"
completed = false
""")
        
        # Create version directory
        version_dir = config_dir / "GAMEID"
        version_dir.mkdir()
        
        version_libs = version_dir / "libs.toml"
        version_libs.write_bytes(b"""
[[lib]]
name = "Game"
mw_version = "GC/1.3.2"

[[lib.object]]
name = "main.c"
completed = false
""")
        
        # Load config
        config = load_config("GAMEID", config_dir)
        
        # Verify configuration
        assert config.tools.binutils_tag == "2.42-1"
        assert config.tools.dtk_tag == "v1.8.0"
        assert config.build.linker_version == "GC/1.3.2"
        assert "-mgekko" in config.build.asflags
        assert config.progress_categories["game"] == "Game"
        
        # Verify libraries
        lib_names = [lib.name for lib in config.libs]
        assert "Runtime" in lib_names
        assert "Game" in lib_names


if __name__ == "__main__":
    print("Running TOML configuration system tests...")
    
    test_config_models_import()
    print("  PASS: config_models import")
    
    test_config_loader_import()
    print("  PASS: config_loader import")
    
    test_tool_versions_dataclass()
    print("  PASS: ToolVersions dataclass")
    
    test_build_flags_dataclass()
    print("  PASS: BuildFlags dataclass")
    
    test_object_def_dataclass()
    print("  PASS: ObjectDef dataclass")
    
    test_library_def_dataclass()
    print("  PASS: LibraryDef dataclass")
    
    test_config_loader_initialization()
    print("  PASS: ConfigLoader initialization")
    
    test_config_loader_load_toml()
    print("  PASS: ConfigLoader.load_toml")
    
    test_config_loader_parse_tool_versions()
    print("  PASS: parse_tool_versions")
    
    test_config_loader_parse_build_flags()
    print("  PASS: parse_build_flags")
    
    test_config_loader_parse_libraries()
    print("  PASS: parse_libraries")
    
    test_load_config_integration()
    print("  PASS: load_config integration")
    
    print("\n" + "=" * 50)
    print("ALL TESTS PASSED!")
    print("=" * 50)
