/// Embedded Python HTTP tool server injected into Docker containers.
/// Adapted from baseagent (https://github.com/PlatformNetwork/baseagent).
///
/// Provides structured, token-efficient tools: read_file, list_dir,
/// grep_files, search_files, apply_patch. Default port 8080, overridden via `--port` flag.
pub const TOOL_SERVER_PY: &str = r#####"#!/usr/bin/env python3
"""HTTP tool server for swe-forge Docker containers.
Adapted from baseagent tools. Provides structured file exploration tools
that are more token-efficient than raw shell commands.
"""
import fnmatch, json, os, re, subprocess, sys, tempfile
from http.server import HTTPServer, BaseHTTPRequestHandler
from pathlib import Path
from typing import Any, Dict, List, Optional

CWD = Path("/repo")

def resolve_path(p: str) -> Path:
    path = Path(p)
    if path.is_absolute():
        return path
    return (CWD / path).resolve()

# -- read_file ---------------------------------------------------------------
def handle_read_file(args: Dict[str, Any]) -> Dict[str, Any]:
    file_path = args.get("file_path", "")
    offset = int(args.get("offset", 0))
    limit = args.get("limit")
    if limit is not None:
        limit = int(limit)

    resolved = resolve_path(file_path)
    if not resolved.exists():
        return {"error": f"File not found: {file_path}"}
    if not resolved.is_file():
        return {"error": f"Not a file: {file_path}"}

    try:
        content = resolved.read_text(encoding="utf-8")
    except UnicodeDecodeError:
        try:
            content = resolved.read_bytes().decode("latin-1")
        except Exception as e:
            return {"error": f"Cannot read file: {e}"}
    except Exception as e:
        return {"error": f"Error reading file: {e}"}

    lines = content.splitlines()
    total = len(lines)
    if total == 0:
        return {"output": "(empty file)", "total_lines": 0, "truncated": False}
    if offset >= total:
        return {"error": f"Offset {offset} exceeds total lines {total}"}

    end = total if limit is None else min(offset + limit, total)
    selected = lines[offset:end]
    truncated = end < total
    formatted = [f"L{i}: {line}" for i, line in enumerate(selected, start=offset + 1)]
    return {
        "output": "\n".join(formatted),
        "total_lines": total,
        "shown_lines": len(selected),
        "offset": offset,
        "truncated": truncated,
    }

# -- list_dir ----------------------------------------------------------------
SKIP_DIRS = {".git", "node_modules", "target", "__pycache__", ".venv", "venv",
             ".tox", "dist", "build", ".mypy_cache", ".pytest_cache", ".eggs"}

def handle_list_dir(args: Dict[str, Any]) -> Dict[str, Any]:
    directory_path = args.get("directory_path", ".")
    recursive = bool(args.get("recursive", False))
    include_hidden = bool(args.get("include_hidden", False))

    resolved = resolve_path(directory_path)
    if not resolved.exists():
        return {"error": f"Directory not found: {directory_path}"}
    if not resolved.is_dir():
        return {"error": f"Not a directory: {directory_path}"}

    def should_skip(name: str) -> bool:
        if not include_hidden and name.startswith("."):
            return True
        return name in SKIP_DIRS

    items: List[tuple] = []
    def collect(path: Path, prefix: str = ""):
        try:
            for entry in sorted(path.iterdir(), key=lambda e: (not e.is_dir(), e.name.lower())):
                if should_skip(entry.name):
                    continue
                rel = f"{prefix}{entry.name}" if prefix else entry.name
                if entry.is_dir():
                    items.append(("dir", rel))
                    if recursive:
                        collect(entry, prefix=f"{rel}/")
                else:
                    items.append(("file", rel))
        except PermissionError:
            pass

    collect(resolved)
    if not items:
        return {"output": f"Directory '{directory_path}' is empty."}
    lines = [f"{t} {n}" for t, n in items]
    return {"output": "\n".join(lines), "count": len(items)}

# -- grep_files --------------------------------------------------------------
def handle_grep_files(args: Dict[str, Any]) -> Dict[str, Any]:
    pattern = args.get("pattern", "")
    include = args.get("include")
    path = args.get("path")
    limit = min(int(args.get("limit", 100)), 2000)

    search_path = resolve_path(path) if path else CWD
    if not search_path.exists():
        return {"error": f"Path not found: {search_path}"}

    # Try ripgrep
    cmd = ["rg", "-n", "--no-heading", "--color=never"]
    if include:
        cmd.extend(["--glob", include])
    cmd.extend(["-m", str(limit), pattern, str(search_path)])
    try:
        result = subprocess.run(cmd, capture_output=True, text=True, timeout=30)
        if result.returncode in (0, 1):
            output = result.stdout.strip() if result.stdout.strip() else "No matches found."
            return {"output": output}
    except FileNotFoundError:
        pass
    except subprocess.TimeoutExpired:
        return {"error": "Search timed out after 30s"}

    # Fallback: grep -rn
    cmd = ["grep", "-rn", "--color=never"]
    if include:
        cmd.extend(["--include", include])
    cmd.extend([pattern, str(search_path)])
    try:
        result = subprocess.run(cmd, capture_output=True, text=True, timeout=30)
        if result.returncode in (0, 1):
            lines = result.stdout.strip().split("\n") if result.stdout.strip() else []
            lines = lines[:limit]
            return {"output": "\n".join(lines) if lines else "No matches found."}
    except FileNotFoundError:
        pass
    except subprocess.TimeoutExpired:
        return {"error": "Search timed out after 30s"}

    # Final fallback: Python regex
    try:
        regex = re.compile(pattern)
    except re.error as e:
        return {"error": f"Invalid regex: {e}"}

    results = []
    def search_dir(d: Path):
        if len(results) >= limit:
            return
        try:
            for item in sorted(d.iterdir()):
                if len(results) >= limit:
                    return
                if item.name.startswith("."):
                    continue
                if item.is_file():
                    if include and not fnmatch.fnmatch(item.name, include):
                        continue
                    try:
                        text = item.read_text(encoding="utf-8", errors="ignore")
                        for i, line in enumerate(text.splitlines(), 1):
                            if regex.search(line):
                                results.append(f"{item}:{i}:{line}")
                                if len(results) >= limit:
                                    return
                    except (PermissionError, OSError):
                        pass
                elif item.is_dir() and not item.is_symlink():
                    search_dir(item)
        except PermissionError:
            pass

    search_dir(search_path)
    return {"output": "\n".join(results) if results else "No matches found."}

# -- search_files ------------------------------------------------------------
DEFAULT_SKIP = {".git", "node_modules", "target", "__pycache__", ".venv", "venv",
                ".tox", "dist", "build", ".mypy_cache", ".pytest_cache"}

def handle_search_files(args: Dict[str, Any]) -> Dict[str, Any]:
    pattern = args.get("pattern", "")
    path = args.get("path", ".")

    resolved = resolve_path(path)
    if not resolved.exists():
        return {"error": f"Path not found: {path}"}

    matches = []
    for root, dirs, files in os.walk(resolved):
        dirs[:] = [d for d in dirs if not d.startswith(".") and d not in DEFAULT_SKIP]
        root_path = Path(root)
        for fname in files:
            if fname.startswith("."):
                continue
            rel = str((root_path / fname).relative_to(resolved))
            if fnmatch.fnmatch(rel, pattern) or fnmatch.fnmatch(fname, pattern):
                matches.append(rel)

    matches.sort()
    if not matches:
        return {"output": f"No files matching '{pattern}'"}
    return {"output": "\n".join(matches), "count": len(matches)}

# -- apply_patch -------------------------------------------------------------
def handle_apply_patch(args: Dict[str, Any]) -> Dict[str, Any]:
    patch = args.get("patch", "")
    if not patch:
        return {"error": "Missing required parameter: patch"}

    try:
        with tempfile.NamedTemporaryFile(mode="w", suffix=".patch", dir=str(CWD), delete=False) as f:
            f.write(patch)
            patch_file = f.name

        result = subprocess.run(
            ["git", "apply", "--allow-empty", patch_file],
            cwd=str(CWD), capture_output=True, text=True, timeout=30
        )
        os.unlink(patch_file)
        if result.returncode == 0:
            return {"output": "Patch applied successfully."}

        with tempfile.NamedTemporaryFile(mode="w", suffix=".patch", dir=str(CWD), delete=False) as f:
            f.write(patch)
            patch_file = f.name

        result = subprocess.run(
            ["git", "apply", "--3way", patch_file],
            cwd=str(CWD), capture_output=True, text=True, timeout=30
        )
        os.unlink(patch_file)
        if result.returncode == 0:
            return {"output": "Patch applied successfully (3-way merge)."}

        return {"error": f"git apply failed: {result.stderr.strip()}"}
    except subprocess.TimeoutExpired:
        return {"error": "Patch apply timed out after 30s"}
    except Exception as e:
        return {"error": f"Failed to apply patch: {e}"}

# -- HTTP Server -------------------------------------------------------------
HANDLERS = {
    "read_file": handle_read_file,
    "list_dir": handle_list_dir,
    "grep_files": handle_grep_files,
    "search_files": handle_search_files,
    "apply_patch": handle_apply_patch,
}

class ToolHandler(BaseHTTPRequestHandler):
    def log_message(self, format, *args):
        pass

    def do_POST(self):
        tool_name = self.path.lstrip("/")
        content_length = int(self.headers.get("Content-Length", 0))
        body = self.rfile.read(content_length).decode("utf-8") if content_length else "{}"
        try:
            args = json.loads(body) if body.strip() else {}
        except json.JSONDecodeError as e:
            self._respond(400, {"error": f"Invalid JSON: {e}"})
            return

        handler = HANDLERS.get(tool_name)
        if not handler:
            self._respond(404, {"error": f"Unknown tool: {tool_name}"})
            return
        try:
            result = handler(args)
            self._respond(200, result)
        except Exception as e:
            self._respond(500, {"error": f"Tool error: {e}"})

    def do_GET(self):
        if self.path == "/health":
            self._respond(200, {"status": "ok"})
        else:
            self._respond(404, {"error": "Not found"})

    def _respond(self, code: int, data: dict):
        body = json.dumps(data).encode("utf-8")
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

def main():
    global CWD
    port = 8080
    for i, arg in enumerate(sys.argv[1:], 1):
        if arg == "--port" and i < len(sys.argv) - 1:
            port = int(sys.argv[i + 1])
        elif arg == "--cwd" and i < len(sys.argv) - 1:
            CWD = Path(sys.argv[i + 1])

    HTTPServer.allow_reuse_address = True
    try:
        server = HTTPServer(("127.0.0.1", port), ToolHandler)
    except OSError as e:
        print(f"FATAL: Cannot bind to port {port}: {e}", flush=True)
        sys.exit(1)
    print(f"Tool server listening on port {port}, cwd={CWD}", flush=True)
    server.serve_forever()

if __name__ == "__main__":
    main()
"#####;
