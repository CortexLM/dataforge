# Kong/deck-1841 (original PR)

Kong/deck (#1841): fix: json summary output and dropped events addition

Due to the way we were handling json output earlier, it showed false summary output if an
upstream error occurred. The user didn't see what operations were performed on the gateway
as the summary showed 0 for all ops.
This is fixed in this PR. Now, json output is similar to yaml output in terms of summary printing.

Further, we have added the new fields added in GDR for dropped operations.
https://github.com/Kong/go-database-reconciler/pull/362

Added a unit test for json output. At the moment, we can't simulate error in
performDiff that can fill Dropped operations. 
One way was to set a negative parallelism to trigger this [error](https://github.com/Kong/go-database-reconciler/blob/main/pkg/diff/diff.go#L463).
However, there's a bug in go-database-reconciler where Run() returns early on
parallelism < 1 without closing channels, causing Solve() to hang when
it tries to range over sc.eventChan.
Captured the bug here: https://github.com/Kong/go-database-reconciler/issues/375
Not prioritising this or the error test yet as this is not a burning issue.

For https://github.com/Kong/deck/issues/1854
