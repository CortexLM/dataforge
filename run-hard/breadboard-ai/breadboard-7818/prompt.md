# breadboard-ai/breadboard-7818

breadboard-ai/breadboard (#7818): graph store no more

Update the editor to eliminate the GraphStore concept. The editor should construct and manage a MutableGraph directly during initialization, and any public API or behavior that previously exposed GraphStore (including its types property or EventTarget-like behavior) should be removed. Ensure the resulting behavior uses MutableGraph without requiring or referencing GraphStore.
