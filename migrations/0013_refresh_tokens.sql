-- Refresh tokens of the phone app (server-api.md section 3: /auth/anonymous, /auth/refresh).
-- A refresh token is used once: a refresh revokes it and stores a new one. Only its hash is kept.

CREATE TABLE refresh_tokens (
    id         uuid PRIMARY KEY,
    -- a deleted user takes the logins with them
    user_id    uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    -- the `deviceId` the app sent when it logged in: which phone this login belongs to
    device_id  TEXT NOT NULL,
    -- sha256 (hex) of the token. The token itself is never stored
    token_hash TEXT NOT NULL UNIQUE,
    expires_at TIMESTAMPTZ NOT NULL,
    -- set when the token was used for a refresh (or revoked with its device); NULL while it works
    revoked_at TIMESTAMPTZ
);

-- "revoke every token of this user on this device" (a revoked token was sent again)
CREATE INDEX refresh_tokens_user_device ON refresh_tokens (user_id, device_id);
