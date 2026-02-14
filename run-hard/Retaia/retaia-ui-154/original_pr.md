# Retaia/retaia-ui-154 (original PR)

Retaia/retaia-ui (#154): feat(auth): gate 2FA by global and user feature state

## Summary
- implement interactive user auth for UI API mode (login/logout/current user)
- gate 2FA UI by runtime feature state from /auth/me/features (global + user effective state)
- add user-level 2FA feature toggle and TOTP setup/enable/disable actions
- keep OTP non-systematic: prompted on login when MFA_REQUIRED, and managed in dedicated 2FA section only when feature is effectively enabled

## Important behavior
- 2FA controls are not always shown
- visibility depends on effective_feature_enabled and user preference, loaded from /auth/me/features
- login remains compatible with accounts without 2FA (no OTP required unless server returns MFA_REQUIRED)

## Validation
- npm run test -- src/api/client.test.ts src/App.test.tsx
- npm run qa
- npm run visual:test
- npm run qa:v1:go-no-go
