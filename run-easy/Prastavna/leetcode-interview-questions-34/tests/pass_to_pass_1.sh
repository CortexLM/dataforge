#!/bin/bash
# This test must PASS on base commit AND after fix
python3 - <<'PY'
import json
from pathlib import Path
path = Path('src/main/web/public/interviews.json')
data = json.loads(path.read_text())
assert isinstance(data, list)
assert len(data) > 0
first = data[0]
for key in ('id','company','rounds','date'):
    assert key in first and first[key]
print('ok')
PY
