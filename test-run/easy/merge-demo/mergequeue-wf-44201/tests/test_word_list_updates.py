import sys
import types
import unittest
from pathlib import Path


# Ensure common utility is importable
UV_LIB_PATH = Path(__file__).resolve().parents[1] / "uv" / "lib"
sys.path.insert(0, str(UV_LIB_PATH))

import common


def load_words_from_nx(txt_filename, nx_subdir):
    """Load words from an nx/<subdir>/*.txt file using common.load_words_from_file.

    We compile a helper module with a fake filename pointing inside the nx
    directory so common.load_words_from_file resolves the correct txt path
    at runtime without reading files directly in the test.
    """
    fake_module_path = Path(__file__).resolve().parents[1] / "nx" / nx_subdir / "_loader.py"
    module_code = """
from common import load_words_from_file

def get_words():
    return load_words_from_file({txt_filename!r})
""".format(txt_filename=txt_filename)

    module = types.ModuleType("nx_loader")
    exec(compile(module_code, str(fake_module_path), "exec"), module.__dict__)
    return module.get_words()


class TestWordListUpdates(unittest.TestCase):
    def _assert_word_update(self, nx_subdir, expected_new, expected_old, stable_word):
        words = load_words_from_nx(f"{nx_subdir}.txt", nx_subdir)

        # New entries should be present and old versions removed
        self.assertIn(expected_new, words)
        self.assertNotIn(expected_old, words)

        # Unrelated word remains to guard against hardcoded replacements
        self.assertIn(stable_word, words)
        self.assertGreater(len(words), 0)

    def test_delta_word_list_update(self):
        self._assert_word_update("delta", "deboshment 1", "deboshment", "dook")

    def test_golf_word_list_update(self):
        self._assert_word_update("golf", "gawsy 1", "gawsy", "guyer")


if __name__ == "__main__":
    unittest.main()
