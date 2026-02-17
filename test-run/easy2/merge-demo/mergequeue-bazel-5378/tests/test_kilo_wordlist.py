from pathlib import Path
import unittest


def parse_wordlist(path: Path):
    entries = []
    for line in path.read_text(encoding="utf-8").splitlines():
        if not line:
            continue
        parts = line.split(" ")
        if len(parts) == 1:
            entries.append((parts[0], None))
        else:
            entries.append((" ".join(parts[:-1]), int(parts[-1])))
    return entries


class TestKiloWordlist(unittest.TestCase):
    def test_kilo_wordlist_requires_suffix_for_koaews(self):
        wordlist_path = Path("bazel/kilo/kilo.txt")
        entries = parse_wordlist(wordlist_path)

        koaews_entries = [suffix for word, suffix in entries if word == "koaews"]
        # The word should appear exactly once and now requires a numeric suffix.
        self.assertEqual(koaews_entries, [1])

        # Neighboring words remain unsuffixed to ensure parsing behavior is correct.
        self.assertTrue(any(word == "koaed" and suffix is None for word, suffix in entries))
        self.assertTrue(
            any(word == "koaejgilnvrz" and suffix is None for word, suffix in entries)
        )

    def test_kilo_wordlist_parses_mixed_suffixes(self):
        wordlist_path = Path("bazel/kilo/kilo.txt")
        entries = parse_wordlist(wordlist_path)

        # Ensure we have a healthy mix of suffixed and unsuffixed words.
        self.assertGreater(len(entries), 1000)
        self.assertTrue(any(suffix is None for _, suffix in entries))
        self.assertTrue(any(suffix is not None for _, suffix in entries))

        # Verify specific non-diff words parse correctly across suffix types.
        expected_samples = {
            "kabxckokr": 1,
            "kaxlmlf": 2,
            "kivgvgbp": 3,
        }
        for word, expected_suffix in expected_samples.items():
            self.assertIn((word, expected_suffix), entries)

        # Confirm unsuffixed entry remains intact.
        self.assertIn(("kabxek", None), entries)


if __name__ == "__main__":
    unittest.main()
