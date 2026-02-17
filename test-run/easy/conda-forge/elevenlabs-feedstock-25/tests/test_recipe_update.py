import pathlib
import re


def read_lines():
    return pathlib.Path("recipe/recipe.yaml").read_text().splitlines()


def get_block(lines, block_name):
    start = None
    for i, line in enumerate(lines):
        if line.startswith(f"{block_name}:"):
            start = i + 1
            break
    if start is None:
        raise AssertionError(f"Missing block: {block_name}")
    block = []
    for line in lines[start:]:
        if line and not line.startswith(" "):
            break
        block.append(line)
    return block


def parse_simple_block(block_lines, indent=2):
    data = {}
    pattern = re.compile(rf"^\s{{{indent}}}([A-Za-z0-9_]+):\s*(.+)$")
    for line in block_lines:
        match = pattern.match(line)
        if match:
            value = match.group(2).strip().strip('"')
            data[match.group(1)] = value
    return data


def parse_run_requirements(lines):
    req_block = get_block(lines, "requirements")
    run_start = None
    for i, line in enumerate(req_block):
        if line.strip() == "run:":
            run_start = i + 1
            break
    if run_start is None:
        raise AssertionError("Missing run requirements")

    run_reqs = []
    for line in req_block[run_start:]:
        if line.startswith("  ") and not line.startswith("    "):
            break
        match = re.match(r"^\s{4}-\s+(.+)$", line)
        if match:
            run_reqs.append(match.group(1).strip())
    return run_reqs


def test_updated_version_and_source_metadata():
    lines = read_lines()
    context_block = parse_simple_block(get_block(lines, "context"))
    source_block = parse_simple_block(get_block(lines, "source"))

    assert context_block["name"] == "elevenlabs"
    assert context_block["version"] == "2.36.0"

    source_url = source_block["url"]
    assert source_url.startswith("https://pypi.org/packages/source/")
    assert "elevenlabs-${{ version }}.tar.gz" in source_url

    sha256 = source_block["sha256"]
    assert len(sha256) == 64
    assert sha256 == "d35e75395caefe97d4e71bd1fd616ba3695a268dee787af064d410177d506106"


def test_run_requirements_include_python_and_httpx():
    lines = read_lines()
    run_reqs = parse_run_requirements(lines)

    assert any(req.startswith("python >=") for req in run_reqs)
    assert "httpx >=0.21.2" in run_reqs
