# merge-demo/mergequeue-bazel-5345

merge-demo/mergequeue-bazel (#5345): [main] exhilarative, chessylite

Update the merge-queue tool configuration to reflect new operational parameters: set the flake rate to 0.1, introduce a logical conflict frequency of 1 per 100, sleep for 600 seconds, close stale items after 48 hours, cap pull request processing at 30 requests per hour targeting the main branch, and include dependencies c and e. Ensure the tool behaves accordingly with these settings.
