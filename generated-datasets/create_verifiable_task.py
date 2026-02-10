#!/usr/bin/env python3
"""
Creates VERIFIABLE tasks with actual files to work on.

This script generates simple tasks that:
1. Have real files (logs, configs, data)
2. Have a clear problem to solve
3. Can be automatically verified

Usage:
    python create_verifiable_task.py [task_type] [output_dir]
    
Task types:
    - log-analysis: Find errors in log files
    - config-fix: Fix a broken config file
    - data-processing: Process CSV data
    - file-search: Find specific files
"""

import os
import sys
import json
import random
import string
import hashlib
from pathlib import Path
from datetime import datetime, timedelta
from typing import Dict, Any, List

# Generate unique canary token
def generate_canary():
    random_part = ''.join(random.choices(string.hexdigits.upper(), k=8))
    return f"DATAFORGE_CANARY_{random_part}"


class LogAnalysisTask:
    """Task: Find the error in a log file."""
    
    def __init__(self, task_id: str):
        self.task_id = task_id
        self.canary = generate_canary()
        self.error_line = random.randint(100, 900)
        self.error_code = random.choice(["500", "502", "503", "504"])
        self.request_id = f"req-{random.randint(10000000, 99999999)}"
        
    def generate_log_file(self, num_lines: int = 1000) -> str:
        """Generate a log file with one error."""
        lines = []
        base_time = datetime.now() - timedelta(hours=24)
        
        for i in range(1, num_lines + 1):
            timestamp = (base_time + timedelta(seconds=i*2)).strftime("%Y-%m-%d %H:%M:%S.%f")[:-3]
            
            if i == self.error_line:
                # The error line
                lines.append(
                    f"{timestamp} ERROR [api-service] HTTP {self.error_code} Connection refused "
                    f"request_id={self.request_id} duration_ms=30001 {self.canary}"
                )
            else:
                # Normal log lines
                level = random.choices(["DEBUG", "INFO", "INFO", "INFO", "WARN"], weights=[1, 4, 4, 4, 1])[0]
                messages = [
                    "Request processed successfully",
                    "Cache hit for user session",
                    "Database query completed",
                    "Health check passed",
                    "Connection pool acquired",
                    "Metrics exported",
                ]
                msg = random.choice(messages)
                req_id = f"req-{random.randint(10000000, 99999999)}"
                duration = random.randint(5, 200)
                lines.append(f"{timestamp} {level} [api-service] {msg} request_id={req_id} duration_ms={duration}")
        
        return "\n".join(lines)
    
    def get_prompt(self) -> str:
        return f"""# Task: Log Analysis

## Problem Statement

A production API service experienced an outage. The logs are stored at `/workspace/data/app.log`.

Your task:
1. Find the ERROR entry in the log file
2. Identify the HTTP status code of the error
3. Find the request_id associated with the error
4. Write your findings to `/workspace/output/analysis.txt` in this format:
   ```
   error_line: <line_number>
   status_code: <http_code>
   request_id: <the_request_id>
   ```

Reference: {self.canary}
"""

    def get_solution(self) -> str:
        return f"""#!/bin/bash
# Reference solution
grep -n "ERROR" /workspace/data/app.log > /tmp/errors.txt
LINE=$(head -1 /tmp/errors.txt | cut -d: -f1)
STATUS=$(grep "ERROR" /workspace/data/app.log | grep -oP 'HTTP \\K[0-9]+')
REQ_ID=$(grep "ERROR" /workspace/data/app.log | grep -oP 'request_id=\\K[^ ]+')

mkdir -p /workspace/output
cat > /workspace/output/analysis.txt << EOF
error_line: $LINE
status_code: $STATUS
request_id: $REQ_ID
EOF
"""

    def get_verification(self) -> Dict[str, Any]:
        return {
            "automated_checks": [
                {"check_type": "file_exists", "target": "/workspace/output/analysis.txt", "expected": "true"},
                {"check_type": "output_contains", "target": "cat /workspace/output/analysis.txt", "expected": str(self.error_line)},
                {"check_type": "output_contains", "target": "cat /workspace/output/analysis.txt", "expected": self.error_code},
                {"check_type": "output_contains", "target": "cat /workspace/output/analysis.txt", "expected": self.request_id},
            ],
            "success_criteria": [
                f"Correctly identifies error on line {self.error_line}",
                f"Correctly identifies HTTP status code {self.error_code}",
                f"Correctly identifies request_id {self.request_id}",
            ],
            "expected_answers": {
                "error_line": self.error_line,
                "status_code": self.error_code,
                "request_id": self.request_id,
            }
        }
    
    def create_task(self, output_dir: Path):
        """Create all task files."""
        task_dir = output_dir / self.task_id
        task_dir.mkdir(parents=True, exist_ok=True)
        
        # Create data directory with log file
        data_dir = task_dir / "workspace" / "data"
        data_dir.mkdir(parents=True, exist_ok=True)
        (data_dir / "app.log").write_text(self.generate_log_file())
        
        # Create output directory
        (task_dir / "workspace" / "output").mkdir(parents=True, exist_ok=True)
        
        # Create prompt
        (task_dir / "prompt.md").write_text(self.get_prompt())
        
        # Create solution (hidden)
        (task_dir / "solution.sh").write_text(self.get_solution())
        
        # Create task.yaml
        verification = self.get_verification()
        task_yaml = {
            "id": self.task_id,
            "category": "debugging",
            "subcategory": "log-analysis",
            "difficulty": "easy",
            "canary_token": self.canary,
            "verification": verification,
            "workspace_path": str(task_dir / "workspace"),
        }
        
        import yaml
        (task_dir / "task.yaml").write_text(yaml.dump(task_yaml, default_flow_style=False))
        
        # Create Dockerfile
        dockerfile = """FROM ubuntu:24.04
RUN apt-get update && apt-get install -y python3 python3-yaml grep gawk coreutils
WORKDIR /workspace
COPY workspace/data /workspace/data
RUN mkdir -p /workspace/output
CMD ["/bin/bash"]
"""
        (task_dir / "Dockerfile").write_text(dockerfile)
        
        # Create docker-compose.yaml
        compose = f"""version: '3.8'
services:
  task-env:
    build: .
    container_name: dataforge-{self.task_id[:8]}
    volumes:
      - ./workspace/output:/workspace/output
    stdin_open: true
    tty: true
"""
        (task_dir / "docker-compose.yaml").write_text(compose)
        
        # Create run script
        run_script = f"""#!/bin/bash
echo "Starting task environment..."
docker-compose up -d --build
echo ""
echo "Task: Log Analysis"
echo "=================="
cat prompt.md
echo ""
echo "Commands:"
echo "  docker exec -it dataforge-{self.task_id[:8]} bash   # Enter container"
echo "  ./verify.sh                                        # Verify solution"
"""
        (task_dir / "run.sh").write_text(run_script)
        os.chmod(task_dir / "run.sh", 0o755)
        
        # Create verify script
        verify_script = f"""#!/bin/bash
CONTAINER="dataforge-{self.task_id[:8]}"

echo "Verifying solution..."
echo "===================="

# Check file exists
if docker exec $CONTAINER test -f /workspace/output/analysis.txt; then
    echo "[PASS] analysis.txt exists"
else
    echo "[FAIL] analysis.txt not found"
    exit 1
fi

# Check content
CONTENT=$(docker exec $CONTAINER cat /workspace/output/analysis.txt)

if echo "$CONTENT" | grep -q "{self.error_line}"; then
    echo "[PASS] Correct error line: {self.error_line}"
else
    echo "[FAIL] Wrong error line (expected {self.error_line})"
fi

if echo "$CONTENT" | grep -q "{self.error_code}"; then
    echo "[PASS] Correct status code: {self.error_code}"
else
    echo "[FAIL] Wrong status code (expected {self.error_code})"
fi

if echo "$CONTENT" | grep -q "{self.request_id}"; then
    echo "[PASS] Correct request_id: {self.request_id}"
else
    echo "[FAIL] Wrong request_id (expected {self.request_id})"
fi

echo ""
echo "Expected answers:"
echo "  error_line: {self.error_line}"
echo "  status_code: {self.error_code}"
echo "  request_id: {self.request_id}"
"""
        (task_dir / "verify.sh").write_text(verify_script)
        os.chmod(task_dir / "verify.sh", 0o755)
        
        print(f"Task created: {task_dir}")
        return task_dir


class ConfigFixTask:
    """Task: Fix a broken JSON config file."""
    
    def __init__(self, task_id: str):
        self.task_id = task_id
        self.canary = generate_canary()
        self.error_type = random.choice(["missing_comma", "missing_quote", "missing_bracket"])
        
    def generate_broken_config(self) -> tuple:
        """Generate a broken config and the fix."""
        config = {
            "server": {
                "host": "0.0.0.0",
                "port": 8080,
                "timeout": 30
            },
            "database": {
                "host": "localhost",
                "port": 5432,
                "name": "myapp_db"
            },
            "logging": {
                "level": "info",
                "format": "json"
            },
            "canary": self.canary
        }
        
        # Create broken version
        good_json = json.dumps(config, indent=2)
        
        if self.error_type == "missing_comma":
            # Remove a comma after "port": 8080
            broken_json = good_json.replace('"port": 8080,', '"port": 8080')
            fix_description = 'Add missing comma after "port": 8080'
            line_number = 4
        elif self.error_type == "missing_quote":
            # Remove closing quote from "localhost"
            broken_json = good_json.replace('"localhost"', '"localhost')
            fix_description = 'Add missing closing quote for "localhost"'
            line_number = 8
        else:  # missing_bracket
            # Remove closing brace of server block
            broken_json = good_json.replace('  },\n  "database"', '  \n  "database"')
            fix_description = 'Add missing closing brace for "server" block'
            line_number = 5
            
        return broken_json, good_json, fix_description, line_number
    
    def get_prompt(self) -> str:
        _, _, fix_desc, _ = self.generate_broken_config()
        return f"""# Task: Fix Configuration File

## Problem Statement

The application fails to start because the configuration file at `/workspace/data/config.json` has a syntax error.

Your task:
1. Identify the JSON syntax error
2. Fix the configuration file
3. Verify it's valid JSON
4. The fixed file should be saved at `/workspace/output/config.json`

Hint: Use `python3 -m json.tool` to validate JSON.

Reference: {self.canary}
"""

    def get_verification(self) -> Dict[str, Any]:
        return {
            "automated_checks": [
                {"check_type": "file_exists", "target": "/workspace/output/config.json", "expected": "true"},
                {"check_type": "output_contains", "target": "python3 -m json.tool /workspace/output/config.json 2>&1", "expected": "server"},
            ],
            "success_criteria": [
                "Fixed configuration file is valid JSON",
                "All original configuration values are preserved",
            ]
        }
    
    def create_task(self, output_dir: Path):
        """Create all task files."""
        task_dir = output_dir / self.task_id
        task_dir.mkdir(parents=True, exist_ok=True)
        
        broken_json, good_json, fix_desc, line_num = self.generate_broken_config()
        
        # Create data directory with broken config
        data_dir = task_dir / "workspace" / "data"
        data_dir.mkdir(parents=True, exist_ok=True)
        (data_dir / "config.json").write_text(broken_json)
        
        # Create output directory
        (task_dir / "workspace" / "output").mkdir(parents=True, exist_ok=True)
        
        # Create prompt
        (task_dir / "prompt.md").write_text(self.get_prompt())
        
        # Create solution
        solution = f"""#!/bin/bash
# The fix: {fix_desc}
cat > /workspace/output/config.json << 'EOF'
{good_json}
EOF
"""
        (task_dir / "solution.sh").write_text(solution)
        
        # Create verify script
        verify_script = f"""#!/bin/bash
CONTAINER="dataforge-{self.task_id[:8]}"

echo "Verifying solution..."
echo "===================="

# Check file exists
if docker exec $CONTAINER test -f /workspace/output/config.json; then
    echo "[PASS] config.json exists"
else
    echo "[FAIL] config.json not found"
    exit 1
fi

# Validate JSON
if docker exec $CONTAINER python3 -m json.tool /workspace/output/config.json > /dev/null 2>&1; then
    echo "[PASS] Valid JSON syntax"
else
    echo "[FAIL] Invalid JSON syntax"
    exit 1
fi

# Check canary token preserved
if docker exec $CONTAINER cat /workspace/output/config.json | grep -q "{self.canary}"; then
    echo "[PASS] Configuration content preserved"
else
    echo "[FAIL] Configuration content missing or corrupted"
fi

echo ""
echo "The fix was: {fix_desc}"
"""
        (task_dir / "verify.sh").write_text(verify_script)
        os.chmod(task_dir / "verify.sh", 0o755)
        
        # Create Dockerfile and docker-compose (similar to LogAnalysisTask)
        dockerfile = """FROM ubuntu:24.04
RUN apt-get update && apt-get install -y python3 jq
WORKDIR /workspace
COPY workspace/data /workspace/data
RUN mkdir -p /workspace/output
CMD ["/bin/bash"]
"""
        (task_dir / "Dockerfile").write_text(dockerfile)
        
        compose = f"""version: '3.8'
services:
  task-env:
    build: .
    container_name: dataforge-{self.task_id[:8]}
    volumes:
      - ./workspace/output:/workspace/output
    stdin_open: true
    tty: true
"""
        (task_dir / "docker-compose.yaml").write_text(compose)
        
        print(f"Task created: {task_dir}")
        return task_dir


def main():
    try:
        import yaml
    except ImportError:
        print("Error: PyYAML required. Install with: pip install pyyaml")
        sys.exit(1)
    
    task_types = {
        "log-analysis": LogAnalysisTask,
        "config-fix": ConfigFixTask,
    }
    
    if len(sys.argv) < 2 or sys.argv[1] in ["-h", "--help"]:
        print(__doc__)
        print("\nAvailable task types:")
        for t in task_types:
            print(f"  - {t}")
        sys.exit(0)
    
    task_type = sys.argv[1]
    output_dir = Path(sys.argv[2]) if len(sys.argv) > 2 else Path("./verifiable-tasks")
    
    if task_type not in task_types:
        print(f"Unknown task type: {task_type}")
        print(f"Available: {list(task_types.keys())}")
        sys.exit(1)
    
    # Generate task ID
    task_id = f"{task_type}-{datetime.now().strftime('%Y%m%d-%H%M%S')}"
    
    # Create task
    task_class = task_types[task_type]
    task = task_class(task_id)
    task_dir = task.create_task(output_dir)
    
    print(f"\n{'='*50}")
    print("TASK CREATED SUCCESSFULLY")
    print(f"{'='*50}")
    print(f"\nTo run the task:")
    print(f"  cd {task_dir}")
    print(f"  ./run.sh")
    print(f"\nTo verify after solving:")
    print(f"  ./verify.sh")


if __name__ == "__main__":
    main()
