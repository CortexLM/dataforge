# stellatogrp/cvxro-56 (original PR)

stellatogrp/cvxro (#56): Optimize reshape_tensor (35-280x speedup) and add profiling suite

## Summary (to be merged after #55 after setting target branch to `develop`)

- **Fix `reshape_tensor()` bottleneck**: Replace the Python-level row-by-row `lil_matrix` copy loop with vectorized NumPy permutation-index + sparse advanced indexing. This was the dominant bottleneck in the canonicalization pipeline, accounting for 60-76% of total `solve()` time at larger problem sizes.

  | Size | Before | After | Speedup |
  |------|--------|-------|---------|
  | n=5 | 1.08ms | 0.031ms | **35x** |
  | n=50 | 15.3ms | 0.054ms | **283x** |
  | n=200 | 381ms | 1.9ms | **200x** |

- **Add `profiling/` suite** with reusable scripts for ongoing performance analysis:
  - `profile_canonicalization.py` — cProfile across 7 problem configurations
  - `profile_memory.py` — tracemalloc per-stage memory analysis
  - `profile_scaling.py` — scaling curves with dimension/constraints/params
  - `profile_hotspots.py` — targeted micro-benchmarks of suspected bottlenecks
  - `run_profiling.sh` — runner script

## Test plan

- [x] All 91 core + integration tests pass
- [x] Run `bash profiling/run_profiling.sh` to verify profiling scripts
