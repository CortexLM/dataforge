import unittest
from pathlib import Path


def load_words(path: Path):
    """Load word list from a text file, stripping whitespace and ignoring blanks."""
    contents = path.read_text(encoding="utf-8").splitlines()
    words = [line.strip() for line in contents if line.strip()]
    return words


class TestBazelWordlists(unittest.TestCase):
    def test_bazel_echo_wordlist_updated_entry(self):
        echo_path = Path("bazel/echo/echo.txt")
        words = load_words(echo_path)

        # Ensure updated entry exists and old entry is removed
        self.assertIn("eluxate 1", words)
        self.assertNotIn("eluxate", words)

        # Make sure other known words still load correctly
        self.assertIn("ependytes", words)
        self.assertNotIn("", words)

        # Only the updated variant should appear for this prefix
        eluxate_variants = [word for word in words if word.startswith("eluxate")]
        self.assertEqual(eluxate_variants, ["eluxate 1"])

    def test_bazel_kilo_wordlist_updated_entry(self):
        kilo_path = Path("bazel/kilo/kilo.txt")
        words = load_words(kilo_path)

        # Ensure updated entry exists and old entry is removed
        self.assertIn("kojkmlak 1", words)
        self.assertNotIn("kojkmlak", words)

        # Make sure other known words still load correctly
        self.assertIn("kojl", words)
        self.assertNotIn("", words)

        # Only the updated variant should appear for this prefix
        kojkmlak_variants = [word for word in words if word.startswith("kojkmlak")]
        self.assertEqual(kojkmlak_variants, ["kojkmlak 1"])


if __name__ == "__main__":
    unittest.main()
