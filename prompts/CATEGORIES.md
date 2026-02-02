# Prompt Categories

Detailed description of all prompt categories in the LLM agent testing library.

## Algorithms (10 prompts)

Tasks focused on algorithm design, implementation, and debugging.

### Graph Algorithms
- **dijkstra-trap.yaml**: Shortest path with corrupted edge weights and Unicode parsing issues
- **negative-cycles.yaml**: Hidden negative cycles with floating-point precision traps
- **disconnected-trap.yaml**: Graph traversal with hidden disconnected components
- **duplicate-edges.yaml**: MST with duplicate/conflicting edges

### Dynamic Programming
- **memoization-trap.yaml**: Cache corruption from mutable defaults and global state
- **state-explosion.yaml**: State space reduction to avoid memory exhaustion

### Sorting & Search
- **comparison-trap.yaml**: Non-transitive comparator causing inconsistent sorting
- **binary-search-edge.yaml**: Off-by-one errors and boundary conditions
- **encoding-trap.yaml**: Unicode normalization issues in string processing

### Complexity Analysis
- **complexity-analysis.yaml**: Hidden quadratic complexity in seemingly linear code

---

## Systems (10 prompts)

Low-level systems programming challenges.

### Operating Systems
- **process-zombie.yaml**: Zombie process accumulation and proper SIGCHLD handling
- **file-descriptor-leak.yaml**: FD exhaustion from error paths and subprocess pipes
- **permission-escalation.yaml**: Setuid binary privilege escalation vulnerabilities

### Distributed Systems
- **split-brain.yaml**: Network partition handling and quorum configuration
- **clock-skew.yaml**: Time synchronization and hybrid logical clocks
- **consensus-failure.yaml**: Raft/Paxos implementation bugs

### Memory Management
- **fragmentation.yaml**: Memory allocator fragmentation and compaction
- **use-after-free.yaml**: Dangling pointers and callback lifecycle issues

### Concurrency
- **deadlock-hidden.yaml**: Transitive deadlocks and RW lock upgrades
- **race-condition.yaml**: Lost updates, torn reads, and TOCTOU bugs

---

## Security (10 prompts)

Security vulnerabilities and hardening tasks.

### Cryptography
- **timing-attack.yaml**: Side-channel leaks through comparison timing
- **weak-random.yaml**: Non-cryptographic PRNG and nonce reuse
- **padding-oracle.yaml**: CBC padding oracle and AEAD migration

### Vulnerabilities
- **sql-injection-hidden.yaml**: Second-order and identifier injection
- **xss-polyglot.yaml**: Context-aware encoding and DOM clobbering
- **deserialization.yaml**: Pickle/YAML arbitrary code execution

### Authentication
- **jwt-confusion.yaml**: Algorithm confusion and kid header injection
- **session-fixation.yaml**: Session regeneration and binding

### Exploitation
- **buffer-overflow.yaml**: Stack/heap overflow and integer overflow
- **rop-chain.yaml**: Return-oriented programming mitigations

---

## Databases (8 prompts)

Database design, optimization, and security.

### SQL
- **transaction-deadlock.yaml**: Lock ordering and isolation level issues
- **query-injection-trap.yaml**: Advanced SQL injection vectors
- **migration-corruption.yaml**: Safe schema migrations with rollback

### NoSQL
- **consistency-trap.yaml**: Eventual consistency and lost updates
- **sharding-trap.yaml**: Shard key selection and hot shards

### Optimization
- **index-trap.yaml**: Index selection, covering indexes, function-on-column
- **query-plan-trap.yaml**: Parameter sniffing and plan cache issues

### Transactions
- **isolation-levels.yaml**: Phantom reads and serialization failures

---

## DevOps (7 prompts)

Infrastructure, CI/CD, and operational security.

### Containers
- **escape-trap.yaml**: Container breakout via privileged mode and mounts
- **resource-exhaustion.yaml**: OOM kills and resource limits

### CI/CD
- **secret-leak.yaml**: Credential exposure in logs and artifacts
- **dependency-confusion.yaml**: Supply chain attacks and registry priority

### Monitoring
- **log-injection.yaml**: Log forging and Log4j-style attacks

### Infrastructure
- **terraform-drift.yaml**: State drift and configuration management
- **dns-poisoning.yaml**: DNSSEC, subdomain takeover, zone transfer

---

## Networking (3 prompts)

Network protocol implementation and troubleshooting.

### Protocols
- **protocol-parsing-vuln.yaml**: Binary protocol parser with integer overflow, endianness, and checksum bypasses

### Sockets
- **socket-fd-exhaustion.yaml**: File descriptor leaks in error paths, SSL handshake failures, and fork inheritance

### Troubleshooting
- **dns-resolution-loop.yaml**: CNAME loop detection, IPv6 fallback delays, and total timeout enforcement

---

## Web (3 prompts)

Web application development challenges.

### Frontend
- **csp-bypass.yaml**: Content Security Policy misconfigurations, nonce reuse, and base-uri attacks

### Backend
- **ssrf-bypass.yaml**: Server-side request forgery via URL parsing, DNS rebinding, and IPv6 formats

### APIs
- **graphql-injection.yaml**: Query depth attacks, alias confusion, introspection exposure, and error leakage

---

## Machine Learning (3 prompts)

ML system challenges.

### Neural Networks
- **gradient-explosion.yaml**: Training stability with proper weight init, gradient clipping, and mixed precision

### NLP
- **tokenizer-collision.yaml**: Unicode normalization, token collision attacks, and deterministic BPE

### Computer Vision
- **model-inversion.yaml**: Privacy-preserving inference against model inversion and membership inference attacks

---

## Traps (8 prompts)

Special prompts designed specifically to trap LLM agents.

### Data Corruption
- **self-destruct.yaml**: Files that corrupt when opened incorrectly
- **encoding-bomb.yaml**: Zip bombs, billion laughs, deep nesting

### State-Dependent
- **order-matters.yaml**: Operations with hidden dependencies
- **hidden-singleton.yaml**: Shared state causing test pollution

### Timing Attacks
- **race-window.yaml**: TOCTOU vulnerabilities
- **time-bomb.yaml**: Date boundary bugs and timer overflows

### Deceptive Structures
- **symlink-trap.yaml**: Malicious symlinks and path traversal
- **unicode-tricks.yaml**: Homoglyphs, RTL override, zero-width chars

---

## Difficulty Distribution

| Category | Easy | Medium | Hard |
|----------|------|--------|------|
| Algorithms | 0 | 2 | 8 |
| Systems | 0 | 0 | 10 |
| Security | 0 | 0 | 10 |
| Databases | 0 | 0 | 8 |
| DevOps | 0 | 1 | 6 |
| Networking | 0 | 0 | 3 |
| Web | 0 | 0 | 3 |
| Machine Learning | 0 | 0 | 3 |
| Traps | 0 | 0 | 8 |
| **Total** | **0** | **3** | **59** |

## Trap Types Summary

| Trap Type | Count | Description |
|-----------|-------|-------------|
| Data corruption | 8 | Files/data that corrupt on wrong access |
| State dependent | 6 | Order or state affects behavior |
| Timing | 7 | Race conditions and time-based issues |
| Type confusion | 5 | Type mismatches cause bugs |
| Encoding | 4 | Character encoding issues |
| Memory safety | 4 | Use-after-free, leaks |
| Injection | 6 | SQL, XSS, log injection |
| Crypto | 5 | Timing attacks, weak random |
| Configuration | 4 | Misconfiguration vulnerabilities |
| Deceptive | 4 | Visual vs actual content differs |

## Test Coverage Requirements

Each prompt should have tests that cover:

1. **Happy path**: Normal operation works
2. **Edge cases**: Boundary conditions handled
3. **Trap activation**: All traps properly trigger
4. **Error handling**: Failures are graceful
5. **Security**: No exploitable vulnerabilities
