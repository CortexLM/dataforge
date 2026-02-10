#!/bin/bash
# Solution for fc132552-291c-428d-8430-14ed0db1e1b8
# DO NOT DISTRIBUTE WITH BENCHMARK

# Approach: Diagnose inode exhaustion in overlayfs upper directories rather than block-level disk exhaustion. Analyze the interaction between containerd's overlayfs snapshotter implementation, kubelet's byte-oriented ephemeral storage monitoring, and high-density CI workloads generating many small files. Identify that while block storage is available, inode tables in the node filesystem become exhausted due to overlayfs upperdir creation patterns. Remediate through node-level kubelet configuration for inode-based eviction thresholds, containerd snapshotter tuning, and node-level log rotation policies rather than workload modifications.

# Key Insights:
# - Kubelet's ephemeral-storage monitoring tracks bytes used, not inode consumption; overlayfs upperdirs consume inodes for every file operation regardless of file size, causing exhaustion while block capacity remains available
# - Containerd with overlayfs stores active container layers as upperdirs on the node root filesystem; CI workloads (npm install, git clone, compilation) generate millions of small files, consuming inodes at rates exceeding node filesystem limits
# - Standard eviction signals monitor /var/lib/kubelet and /var/lib/containerd, but miss inode pressure in overlay mount propagation paths and container log directories
# - The 'restricted' Pod Security Standard prevents hostPath mounting but does not prevent the node filesystem from serving as the backing store for overlayfs upperdirs, creating an invisible resource boundary violation
# - Containerd log rotation and kubelet log rotation can desynchronize, leaving orphaned log files in /var/log/pods that consume inodes outside of container cgroups' accounting

# Reference Commands:
# Step 1:
df -i /var/lib/containerd /var/lib/kubelet /var/log

# Step 2:
crictl ps -q | xargs -I{} crictl inspect {} | jq '.info.runtimeSpec.root.path'

# Step 3:
find /var/lib/containerd/io.containerd.snapshotter.v1.overlayfs/snapshots -maxdepth 3 -type f | wc -l

# Step 4:
kubectl get events --field-selector reason=Evicted --all-namespaces -o yaml | grep -A5 'ephemeral storage'

# Step 5:
systemctl cat kubelet | grep -E 'eviction|inode'

# Step 6:
cat /etc/containerd/config.toml | grep -A10 'snapshotter'

# Step 7:
mount | grep overlay | head -5

# Step 8:
ls -la /var/log/pods/*/*/*.log* 2>/dev/null | wc -l
