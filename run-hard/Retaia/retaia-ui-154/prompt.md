# Retaia/retaia-ui-154

Retaia/retaia-ui (#154): feat(auth): gate 2FA by global and user feature state

Implement interactive authentication for the UI API mode so users can log in, log out, and fetch the current user. Gate all 2FA UI and management by the effective feature state returned from the user features endpoint (global + user state). Only show 2FA controls when the effective feature is enabled and the user has opted in. Provide user-level 2FA toggling and TOTP setup/enable/disable flows. Ensure login remains compatible with accounts without 2FA: require an OTP only when the server indicates MFA is required, and otherwise allow normal login. Keep 2FA management confined to the dedicated 2FA section when enabled.
