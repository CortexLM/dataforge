# stellatogrp/cvxro-56

stellatogrp/cvxro (#56): Optimize reshape_tensor (35-280x speedup) and add profiling suite

Improve performance of tensor reshaping to remove a major bottleneck in the canonicalization pipeline. Ensure reshaping scales efficiently at larger problem sizes and no longer dominates solve time. Add a profiling suite with scripts to benchmark runtime, memory usage, scaling behavior, and hotspots across multiple configurations, plus a runner to execute the profiling workflows.
