import os
import unittest
from pathlib import Path


class TestClaudeDocumentation(unittest.TestCase):
    def test_claude_doc_exists_and_has_content(self):
        doc_path = Path("CLAUDE.md")
        self.assertTrue(doc_path.exists(), "CLAUDE.md should exist in repo root")
        self.assertTrue(doc_path.is_file(), "CLAUDE.md should be a file")
        # Use file metadata only; ensure it's non-trivial in size
        size = doc_path.stat().st_size
        self.assertGreater(size, 1000, "CLAUDE.md should contain substantial documentation")


if __name__ == "__main__":
    unittest.main()
