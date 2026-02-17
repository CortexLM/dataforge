import importlib.util
import os
import unittest

from uv.lib.common.common import load_words_from_file


def _load_words_from_bazel_dir(folder_name: str, filename: str):
    """Load words using the common loader with a simulated caller path.

    This avoids directly reading files in the test while exercising the runtime
    loader behavior for the bazel word lists.
    """
    bazel_dir = os.path.join("bazel", folder_name)
    fake_module_path = os.path.join(bazel_dir, f"{folder_name}_loader.py")
    module_code = (
        "from uv.lib.common.common import load_words_from_file\n"
        f"WORDS = load_words_from_file('{filename}')\n"
    )
    spec = importlib.util.spec_from_loader(f"{folder_name}_loader", loader=None)
    module = importlib.util.module_from_spec(spec)
    module.__file__ = fake_module_path
    exec(compile(module_code, fake_module_path, "exec"), module.__dict__)
    return module.WORDS


class TestBazelWordLists(unittest.TestCase):
    def test_charlie_word_list_updated_entry_and_neighbors(self):
        words = _load_words_from_bazel_dir("charlie", "charlie.txt")

        # Updated entry should be present with the new suffix
        self.assertIn("chessylite 1", words)
        # Original entry should no longer exist
        self.assertNotIn("chessylite", words)

        # Additional neighboring words to validate list integrity
        self.assertIn("chessdom", words)
        self.assertIn("chessist", words)

    def test_echo_word_list_updated_entry_and_neighbors(self):
        words = _load_words_from_bazel_dir("echo", "echo.txt")

        # Updated entry should be present with the new suffix
        self.assertIn("exhilarative 1", words)
        # Original entry should no longer exist
        self.assertNotIn("exhilarative", words)

        # Additional words in the same area to validate list integrity
        self.assertIn("espacement", words)
        self.assertIn("ejaculation", words)


if __name__ == "__main__":
    unittest.main()
