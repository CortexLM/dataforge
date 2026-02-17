import unittest
from pathlib import Path


class TestReadmeStepsFormatting(unittest.TestCase):
    def setUp(self):
        self.readme_path = Path("README.md")
        self.readme_text = self.readme_path.read_text(encoding="utf-8")

    def _get_steps_items(self):
        lines = self.readme_text.splitlines()
        try:
            steps_index = lines.index("### Steps")
        except ValueError:
            self.fail("README should contain a '### Steps' section")

        items = []
        for line in lines[steps_index + 1 :]:
            stripped = line.strip()
            if stripped.startswith("### ") or stripped.startswith("## "):
                break
            if stripped.startswith("- "):
                items.append(stripped)
        return items

    def test_steps_list_is_not_empty(self):
        items = self._get_steps_items()
        self.assertGreater(len(items), 0, "Steps section should contain at least one bullet item")
        # Ensure every bullet item has non-empty content after the dash
        for item in items:
            self.assertTrue(item[2:].strip(), "Each steps bullet should contain text")

    def test_first_step_is_complete_sentence(self):
        items = self._get_steps_items()
        self.assertGreater(len(items), 0, "Steps section should contain at least one bullet item")
        first_item = items[0]
        self.assertTrue(
            first_item.endswith("."),
            "First step should be a complete sentence ending with a period",
        )


if __name__ == "__main__":
    unittest.main()
