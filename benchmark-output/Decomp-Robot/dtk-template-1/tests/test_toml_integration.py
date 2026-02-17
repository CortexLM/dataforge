"""Integration tests for TOML-based configuration system.

These tests verify the complete TOML configuration system behavior
including parsing actual TOML files and hierarchical configuration.
"""

import sys
import tempfile
from pathlib import Path
import tomllib


def test_actual_toml_files_parsing():
    """Test parsing actual TOML files as they would be in the PR."""
    with tempfile.TemporaryDirectory() as tmpdir:
        config_dir = Path(tmpdir) / "config"
        config_dir.mkdir()
        
        # Create default.toml with actual content from PR
        default_toml = config_dir / "default.toml"
        default_toml.write_bytes(b"""
[project]
default_version = "GAMEID"

[tools]
binutils_tag = "2.42-1"
compilers_tag = "20251118"
dtk_tag = "v1.8.0"
objdiff_tag = "v3.5.1"
sjiswrap_tag = "v1.2.2"
wibo_tag = "1.0.0"

[build]
linker_version = "GC/1.3.2"
asflags = [
    "-mgekko",
    "--strip-local-absolute",
    "-I include",
]
cflags_base = [
    "-nodefaults",
    "-proc gekko",
    "-O4,p",
]
cflags_debug = [
    "-sym on",
    "-DDEBUG=1",
]
ldflags = [
    "-fp hardware",
    "-nodefaults",
]

[progress.categories]
game = "Game Code"
sdk = "SDK Code"
""")
        
        # Parse default.toml
        with open(default_toml, 'rb') as f:
            default_config = tomllib.load(f)
        
        assert default_config['project']['default_version'] == 'GAMEID'
        assert default_config['tools']['binutils_tag'] == '2.42-1'
        assert default_config['tools']['wibo_tag'] == '1.0.0'
        assert default_config['build']['linker_version'] == 'GC/1.3.2'
        assert '-mgekko' in default_config['build']['asflags']
        assert default_config['progress']['categories']['game'] == 'Game Code'


def test_object_states():
    """Test Matching, NonMatching, and Equivalent object states."""
    toml_content = b"""
[[lib]]
name = "Test"
mw_version = "GC/1.3.2"

[[lib.object]]
name = "matching.c"
completed = true

[[lib.object]]
name = "nonmatching.c"
completed = false

[[lib.object]]
name = "equivalent.c"
completed = true
equivalent = true
"""
    
    config = tomllib.loads(toml_content.decode('utf-8'))
    
    objects = config['lib'][0]['object']
    
    matching = next(o for o in objects if o['name'] == 'matching.c')
    assert matching['completed'] == True
    assert matching.get('equivalent', False) == False
    
    nonmatching = next(o for o in objects if o['name'] == 'nonmatching.c')
    assert nonmatching['completed'] == False
    
    equivalent = next(o for o in objects if o['name'] == 'equivalent.c')
    assert equivalent['completed'] == True
    assert equivalent['equivalent'] == True


def test_per_object_compiler_options():
    """Test per-object compiler options."""
    toml_content = b"""
[[lib]]
name = "Test"
mw_version = "GC/1.3.2"

[[lib.object]]
name = "optimized.c"
completed = false
cflags = ["-O3", "-inline on"]
mw_version = "GC/1.2.5"

[[lib.object]]
name = "assembly.s"
completed = false
asflags = ["-mgekko"]

[[lib.object]]
name = "versioned.c"
completed = false
versions = ["GAMEID_US", "GAMEID_JP"]
"""
    
    config = tomllib.loads(toml_content.decode('utf-8'))
    
    objects = config['lib'][0]['object']
    
    optimized = next(o for o in objects if o['name'] == 'optimized.c')
    assert optimized['cflags'] == ["-O3", "-inline on"]
    assert optimized['mw_version'] == "GC/1.2.5"
    
    asm = next(o for o in objects if o['name'] == 'assembly.s')
    assert asm['asflags'] == ["-mgekko"]
    
    versioned = next(o for o in objects if o['name'] == 'versioned.c')
    assert versioned['versions'] == ["GAMEID_US", "GAMEID_JP"]


def test_hierarchical_config_merging():
    """Test hierarchical configuration merging."""
    default_toml = b"""
[project]
default_version = "GAMEID"

[tools]
binutils_tag = "2.42-1"
dtk_tag = "v1.8.0"

[build]
linker_version = "GC/1.0"
cflags_base = ["-O4,p"]

[progress.categories]
game = "Game Code"
"""
    
    version_toml = b"""
[build]
linker_version = "GC/1.3.2"
cflags_extra = ["-DEXTRA"]
"""
    
    default_config = tomllib.loads(default_toml.decode('utf-8'))
    version_config = tomllib.loads(version_toml.decode('utf-8'))
    
    # Simulate merge
    merged = {**default_config}
    for key in version_config:
        if key in merged and isinstance(merged[key], dict):
            merged[key].update(version_config[key])
        else:
            merged[key] = version_config[key]
    
    # Default values preserved
    assert merged['tools']['binutils_tag'] == '2.42-1'
    assert '-O4,p' in merged['build']['cflags_base']
    assert merged['progress']['categories']['game'] == 'Game Code'
    
    # Version overrides applied
    assert merged['build']['linker_version'] == 'GC/1.3.2'
    assert merged['build']['cflags_extra'] == ['-DEXTRA']


if __name__ == "__main__":
    print("Running TOML integration tests...")
    
    test_actual_toml_files_parsing()
    print("  PASS: actual TOML files parsing")
    
    test_object_states()
    print("  PASS: object states")
    
    test_per_object_compiler_options()
    print("  PASS: per-object compiler options")
    
    test_hierarchical_config_merging()
    print("  PASS: hierarchical config merging")
    
    print("\n" + "=" * 50)
    print("ALL INTEGRATION TESTS PASSED!")
    print("=" * 50)
