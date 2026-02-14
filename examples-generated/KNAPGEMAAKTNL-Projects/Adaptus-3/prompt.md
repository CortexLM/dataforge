# KNAPGEMAAKTNL-Projects/Adaptus-3

KNAPGEMAAKTNL-Projects/Adaptus (#3): feat: workout duration, estimated 1RM, and offline queue

## Summary
- **Workout Duration**: Derive from existing timestamps, display avg duration in stats, weekly total on dashboard, per-workout completion time
- **Estimated 1RM**: Epley formula endpoint, e1rm per session in exercise history, electric purple accent card, dual-line progress chart, clickable exercise names in PR list
- **Offline Queue**: Queue failed writes in localStorage with temp ID system, synthetic responses, auto-sync on reconnect, persistent badge, toast notifications, cancel cleanup

## Test plan
- [ ] Complete a workout → "Completed in X min" shows, dashboard/stats show duration
- [ ] Exercise stats page shows estimated 1RM card, chart has two lines with legend
- [ ] Stats page PR list shows e1rm values, exercise names are clickable
- [ ] Turn off network → log sets → "X pending" badge appears
- [ ] Turn on network → queue syncs → badge disappears → data persists
- [ ] Cancel workout while offline → queue cleaned up

🤖 Generated with [Claude Code](https://claude.com/claude-code)
