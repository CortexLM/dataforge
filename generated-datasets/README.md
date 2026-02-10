# Dataforge Generated Datasets

## Tasks Generated

| Task ID | Category | Difficulty | Description |
|---------|----------|------------|-------------|
| `fc132552-291c-428d-8430-14ed0db1e1b8` | containers | Hard | Kubernetes inode exhaustion diagnosis |
| `9e94c50c-f149-4d84-bbc1-413fcbce90de` | networking | Hard | PMTUD black hole in hybrid cloud |
| `f3992eb4-505f-4944-9cca-7971e1c29326` | security | Hard | Side-channel attack on BLS12-381 |
| `9a9e5e56-3cae-4900-95d6-7a905d4830cd` | system-administration | Hard | Conntrack exhaustion P0 incident |
| `8af974f6-4cf8-4793-9732-0fa62346ee59` | software-engineering | Hard | Distributed checkout saga pattern |
| `fbeca9c5-a36e-4cef-8663-0f13b7b139fc` | software-engineering | Hard | Exactly-once distributed task queue |

## Structure

Each task directory contains:
- `task.yaml` - Full task specification with verification criteria
- `prompt.md` - The problem statement for the agent
- `solution.sh` - Reference solution (hidden from agent)

## Verification

### Using the Verification Script

```bash
# Verify a solution against a task
python verify_solution.py <task_id_dir> <your_solution_dir>

# Example
python verify_solution.py fc132552-291c-428d-8430-14ed0db1e1b8 ./my_k8s_solution
```

### Verification Process

1. **Canary Token Check** - Ensures the solution references the unique task ID
2. **Automated Checks** - Runs file existence and content checks from `task.yaml`
3. **Success Criteria** - Displays manual review criteria for human evaluation

### Scoring

- **Automated Score**: Percentage of automated checks passed (target: 70%+)
- **Required Checks**: Must all pass (e.g., canary token)
- **Manual Review**: Most tasks require human verification of solution quality

## Task Details

### Task 1: Kubernetes Inode Exhaustion
Diagnose why pods are evicted for "ephemeral storage exhaustion" when disk is 60% free.
Hidden cause: overlayfs inode exhaustion, not block storage.

### Task 2: PMTUD Black Hole
Debug 30-second delays on large file transfers in hybrid cloud architecture.
Hidden cause: MTU mismatch (9001 vs 1500) with ICMP filtering blocking PMTUD.

### Task 3: Cryptographic Side-Channel
Audit zero-knowledge auth system for cache-timing vulnerabilities in BLS12-381.
Required: Statistical analysis, exploit chain, Spectre v4 evaluation.

### Task 4: Conntrack Table Exhaustion
Resolve intermittent 502 errors on financial platform with Calico CNI.
Hidden cause: payment-gateway creating new TCP connection per request.

### Task 5: Distributed Checkout Architecture
Design saga pattern for e-commerce handling duplicate payments, overselling, orphaned inventory.
Required: Sequence diagrams, idempotency strategy, reconciliation algorithm.

### Task 6: Exactly-Once Task Queue
Design partition-tolerant distributed queue for financial transactions.
Required: Fencing tokens, clock-independent leases, DLQ with ordering.

## Anti-Memorization

Each task contains a unique canary token (e.g., `DATAFORGE_CANARY_CANARY-XXXXXXXX-...`)
that must appear in valid solutions. This prevents model memorization.
