"""Tests for remote_dir and remote_import_path configuration options in modules."""
import pytest
from pathlib import Path


def simple_add(a, b):
    """Simple function for testing."""
    return a + b


class SimpleClass:
    """Simple class for testing."""
    
    def add(self, a, b):
        return a + b


class TestModuleRemoteDir:
    """Test Module class remote_dir and remote_import_path options."""
    
    def test_module_accepts_remote_dir_parameter(self):
        """Test that Module.__init__ accepts remote_dir parameter."""
        from kubetorch.resources.callables.module import Module
        import inspect
        
        sig = inspect.signature(Module.__init__)
        params = list(sig.parameters.keys())
        
        assert 'remote_dir' in params, "Module.__init__ should accept remote_dir parameter"
    
    def test_module_accepts_remote_import_path_parameter(self):
        """Test that Module.__init__ accepts remote_import_path parameter."""
        from kubetorch.resources.callables.module import Module
        import inspect
        
        sig = inspect.signature(Module.__init__)
        params = list(sig.parameters.keys())
        
        assert 'remote_import_path' in params, "Module.__init__ should accept remote_import_path parameter"
    
    def test_module_rejects_sync_dir_and_remote_dir_together(self):
        """Test that Module raises ValueError when both sync_dir and remote_dir are specified."""
        from kubetorch.resources.callables.module import Module
        
        test_file = Path(__file__).resolve()
        pointers = (str(test_file.parent), "test_remote_dir", "simple_add")
        
        with pytest.raises(ValueError, match="sync_dir and remote_dir can not both be set"):
            Module(
                name="test-module",
                pointers=pointers,
                sync_dir="/some/local/path",
                remote_dir="/some/remote/path"
            )
    
    def test_module_rejects_remote_import_path_without_remote_dir(self):
        """Test that Module raises ValueError when remote_import_path is set without remote_dir."""
        from kubetorch.resources.callables.module import Module
        
        test_file = Path(__file__).resolve()
        pointers = (str(test_file.parent), "test_remote_dir", "simple_add")
        
        with pytest.raises(ValueError, match="remote_import_path can only be set when remote_dir is also set"):
            Module(
                name="test-module",
                pointers=pointers,
                remote_import_path="custom.import.path"
            )
    
    def test_module_accepts_only_remote_dir(self):
        """Test that Module accepts remote_dir without remote_import_path."""
        from kubetorch.resources.callables.module import Module
        
        test_file = Path(__file__).resolve()
        pointers = (str(test_file.parent), "test_remote_dir", "simple_add")
        
        module = Module(
            name="test-module",
            pointers=pointers,
            remote_dir="/app/custom/path"
        )
        
        assert module._remote_root_path == "/app/custom/path"
    
    def test_module_accepts_remote_dir_and_remote_import_path(self):
        """Test that Module accepts both remote_dir and remote_import_path together."""
        from kubetorch.resources.callables.module import Module
        
        test_file = Path(__file__).resolve()
        pointers = (str(test_file.parent), "test_remote_dir", "simple_add")
        
        module = Module(
            name="test-module",
            pointers=pointers,
            remote_dir="/app/custom/path",
            remote_import_path="custom.import.path"
        )
        
        assert module._remote_root_path == "/app/custom/path"
        assert module._import_path == "custom.import.path"


class TestClsRemoteDir:
    """Test Cls class remote_dir and remote_import_path options."""
    
    def test_cls_accepts_remote_dir_parameter(self):
        """Test that Cls.__init__ accepts remote_dir parameter."""
        from kubetorch.resources.callables.cls.cls import Cls
        import inspect
        
        sig = inspect.signature(Cls.__init__)
        params = list(sig.parameters.keys())
        
        assert 'remote_dir' in params, "Cls.__init__ should accept remote_dir parameter"
    
    def test_cls_accepts_remote_import_path_parameter(self):
        """Test that Cls.__init__ accepts remote_import_path parameter."""
        from kubetorch.resources.callables.cls.cls import Cls
        import inspect
        
        sig = inspect.signature(Cls.__init__)
        params = list(sig.parameters.keys())
        
        assert 'remote_import_path' in params, "Cls.__init__ should accept remote_import_path parameter"
    
    def test_cls_factory_accepts_remote_dir_parameter(self):
        """Test that cls() factory function accepts remote_dir parameter."""
        from kubetorch.resources.callables.cls.cls import cls as cls_factory
        import inspect
        
        sig = inspect.signature(cls_factory)
        params = list(sig.parameters.keys())
        
        assert 'remote_dir' in params, "cls() factory should accept remote_dir parameter"
    
    def test_cls_factory_accepts_remote_import_path_parameter(self):
        """Test that cls() factory function accepts remote_import_path parameter."""
        from kubetorch.resources.callables.cls.cls import cls as cls_factory
        import inspect
        
        sig = inspect.signature(cls_factory)
        params = list(sig.parameters.keys())
        
        assert 'remote_import_path' in params, "cls() factory should accept remote_import_path parameter"
    
    def test_cls_factory_rejects_sync_dir_and_remote_dir_together(self):
        """Test that cls() factory raises error when both sync_dir and remote_dir are specified."""
        import kubetorch as kt
        
        with pytest.raises(ValueError, match="sync_dir and remote_dir can not both be set"):
            kt.cls(SimpleClass, sync_dir="/local/path", remote_dir="/remote/path")
    
    def test_cls_factory_rejects_remote_import_path_without_remote_dir(self):
        """Test that cls() factory raises error when remote_import_path is set without remote_dir."""
        import kubetorch as kt
        
        with pytest.raises(ValueError, match="remote_import_path can only be set when remote_dir is also set"):
            kt.cls(SimpleClass, remote_import_path="custom.import.path")
    
    def test_cls_factory_accepts_only_remote_dir(self):
        """Test that cls() factory accepts remote_dir without remote_import_path."""
        import kubetorch as kt
        
        remote_cls = kt.cls(SimpleClass, remote_dir="/app/custom/path")
        assert remote_cls._remote_root_path == "/app/custom/path"
    
    def test_cls_factory_accepts_remote_dir_and_remote_import_path(self):
        """Test that cls() factory accepts both remote_dir and remote_import_path."""
        import kubetorch as kt
        
        remote_cls = kt.cls(SimpleClass, remote_dir="/app/custom/path", remote_import_path="custom.module.path")
        assert remote_cls._remote_root_path == "/app/custom/path"
        assert remote_cls._import_path == "custom.module.path"


class TestFnRemoteDir:
    """Test Fn class remote_dir and remote_import_path options."""
    
    def test_fn_accepts_remote_dir_parameter(self):
        """Test that Fn.__init__ accepts remote_dir parameter."""
        from kubetorch.resources.callables.fn.fn import Fn
        import inspect
        
        sig = inspect.signature(Fn.__init__)
        params = list(sig.parameters.keys())
        
        assert 'remote_dir' in params, "Fn.__init__ should accept remote_dir parameter"
    
    def test_fn_accepts_remote_import_path_parameter(self):
        """Test that Fn.__init__ accepts remote_import_path parameter."""
        from kubetorch.resources.callables.fn.fn import Fn
        import inspect
        
        sig = inspect.signature(Fn.__init__)
        params = list(sig.parameters.keys())
        
        assert 'remote_import_path' in params, "Fn.__init__ should accept remote_import_path parameter"
    
    def test_fn_factory_accepts_remote_dir_parameter(self):
        """Test that fn() factory function accepts remote_dir parameter."""
        from kubetorch.resources.callables.fn.fn import fn as fn_factory
        import inspect
        
        sig = inspect.signature(fn_factory)
        params = list(sig.parameters.keys())
        
        assert 'remote_dir' in params, "fn() factory should accept remote_dir parameter"
    
    def test_fn_factory_accepts_remote_import_path_parameter(self):
        """Test that fn() factory function accepts remote_import_path parameter."""
        from kubetorch.resources.callables.fn.fn import fn as fn_factory
        import inspect
        
        sig = inspect.signature(fn_factory)
        params = list(sig.parameters.keys())
        
        assert 'remote_import_path' in params, "fn() factory should accept remote_import_path parameter"
    
    def test_fn_factory_rejects_sync_dir_and_remote_dir_together(self):
        """Test that fn() factory raises error when both sync_dir and remote_dir are specified."""
        import kubetorch as kt
        
        with pytest.raises(ValueError, match="sync_dir and remote_dir can not both be set"):
            kt.fn(simple_add, sync_dir="/local/path", remote_dir="/remote/path")
    
    def test_fn_factory_rejects_remote_import_path_without_remote_dir(self):
        """Test that fn() factory raises error when remote_import_path is set without remote_dir."""
        import kubetorch as kt
        
        with pytest.raises(ValueError, match="remote_import_path can only be set when remote_dir is also set"):
            kt.fn(simple_add, remote_import_path="custom.import.path")
    
    def test_fn_factory_accepts_only_remote_dir(self):
        """Test that fn() factory accepts remote_dir without remote_import_path."""
        import kubetorch as kt
        
        remote_fn = kt.fn(simple_add, remote_dir="/app/custom/path")
        assert remote_fn._remote_root_path == "/app/custom/path"
    
    def test_fn_factory_accepts_remote_dir_and_remote_import_path(self):
        """Test that fn() factory accepts both remote_dir and remote_import_path."""
        import kubetorch as kt
        
        remote_fn = kt.fn(simple_add, remote_dir="/app/custom/path", remote_import_path="custom.module.path")
        assert remote_fn._remote_root_path == "/app/custom/path"
        assert remote_fn._import_path == "custom.module.path"


class TestRemoteDirValidation:
    """Test validation logic for remote_dir and remote_import_path combinations."""
    
    def test_path_object_accepted_as_remote_dir(self):
        """Test that Path objects are accepted for remote_dir."""
        from kubetorch.resources.callables.module import Module
        
        test_file = Path(__file__).resolve()
        pointers = (str(test_file.parent), "test_remote_dir", "simple_add")
        
        remote_dir_path = Path("/app/custom/path")
        module = Module(
            name="test-module",
            pointers=pointers,
            remote_dir=remote_dir_path
        )
        
        assert module._remote_root_path == "/app/custom/path"
    
    def test_different_path_formats_accepted(self):
        """Test various path formats for remote_dir."""
        import kubetorch as kt
        
        remote_fn1 = kt.fn(simple_add, remote_dir="/absolute/path/to/module")
        assert remote_fn1._remote_root_path == "/absolute/path/to/module"
        
        remote_fn2 = kt.fn(simple_add, remote_dir="~/relative/path")
        assert "relative/path" in remote_fn2._remote_root_path
    
    def test_empty_remote_import_path_allowed(self):
        """Test that empty string for remote_import_path is allowed."""
        from kubetorch.resources.callables.module import Module
        
        test_file = Path(__file__).resolve()
        pointers = (str(test_file.parent), "test_remote_dir", "simple_add")
        
        module = Module(
            name="test-module",
            pointers=pointers,
            remote_dir="/app/path",
            remote_import_path=""
        )
        assert module._import_path == "test_remote_dir"


class TestRemoteDirEdgeCases:
    """Test edge cases for remote_dir and remote_import_path."""
    
    def test_none_remote_dir_does_not_set_remote_root_path(self):
        """Test that when remote_dir is None, _remote_root_path is not set at init."""
        from kubetorch.resources.callables.module import Module
        
        test_file = Path(__file__).resolve()
        pointers = (str(test_file.parent), "test_remote_dir", "simple_add")
        
        module = Module(
            name="test-module",
            pointers=pointers,
            remote_dir=None
        )
        
        assert module._remote_root_path is None
    
    def test_remote_dir_with_special_characters(self):
        """Test that remote_dir handles special path characters correctly."""
        import kubetorch as kt
        
        remote_fn = kt.fn(simple_add, remote_dir="/path with spaces/module")
        assert "/path with spaces/module" in remote_fn._remote_root_path
        
        remote_fn2 = kt.fn(simple_add, remote_dir="/my-app_v2/module-dir")
        assert remote_fn2._remote_root_path == "/my-app_v2/module-dir"
