#!/bin/bash
# This test must FAIL on base commit, PASS after fix
python3 - <<'PY'
import json
from pathlib import Path
path = Path('src/main/web/public/interviews.json')
data = json.loads(path.read_text())
assert data[0].get('id') == 'i_W7bfom7GW7gmqwk58KXyj9'
PY
