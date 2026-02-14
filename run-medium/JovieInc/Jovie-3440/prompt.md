# JovieInc/Jovie-3440

JovieInc/Jovie (#3440): Refactor ReleaseSettings to use manual state instead of useTransition

Update the Release Settings UI to manage its pending/loading state explicitly without relying on React transition hooks. The component should continue to show correct loading behavior during async operations, recover state properly on failures, and provide clear success/error feedback to users when configuration changes are applied. Preserve existing user-facing behavior aside from the state management approach.
