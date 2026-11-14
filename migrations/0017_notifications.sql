-- Messages for the user and the phones that get pushes (server-api.md: Notification, POST /devices).

CREATE TABLE notifications (
    -- time-ordered uuid (v7)
    id         uuid PRIMARY KEY,
    -- a deleted user takes the notifications with them
    user_id    uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    kind       TEXT NOT NULL CHECK (kind IN ('delay', 'charge', 'crowding', 'renewal')),
    title      TEXT NOT NULL,
    body       TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    read       BOOLEAN NOT NULL DEFAULT false
);

-- GET /notifications lists the newest first, one user at a time
CREATE INDEX notifications_user_idx ON notifications (user_id, created_at DESC, id DESC);

-- The phones of a user, by their Expo push token. A token belongs to one phone: it is the key, so a phone that
-- is used by another user later moves to that user.
CREATE TABLE push_devices (
    push_token TEXT PRIMARY KEY,
    user_id    uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    platform   TEXT NOT NULL CHECK (platform IN ('ios', 'android'))
);

CREATE INDEX push_devices_user_idx ON push_devices (user_id);
