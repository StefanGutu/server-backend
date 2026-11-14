-- A ride of one user on one bus (server-api.md section 2).
-- Rows are created by the boarding decision (step 12); this step only reads them.

CREATE TABLE ride_sessions (
    id             uuid PRIMARY KEY,
    -- a deleted user takes their rides with them (DELETE /me must keep working)
    user_id        uuid    NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    bus_id         INTEGER NOT NULL REFERENCES buses (bus_id),
    -- "none" is never stored: it means "no row"
    status         TEXT    NOT NULL CHECK (status IN
                   ('pending', 'awaiting_confirmation', 'payment_failed', 'confirmed', 'rejected', 'ended')),
    -- grows at every change: it is the ETag of the ride (step 11)
    version        BIGINT  NOT NULL DEFAULT 1,
    -- [{ "busId", "line" }] when the user hears several buses and the server cannot choose
    candidates     JSONB   NOT NULL DEFAULT '[]',
    passengers     INTEGER NOT NULL DEFAULT 1 CHECK (passengers BETWEEN 1 AND 10),
    -- only when status is payment_failed
    payment_reason TEXT    CHECK (payment_reason IN ('card_declined', 'authentication_required')),
    -- only when payment_reason is authentication_required
    client_secret  TEXT,
    expires_at     TIMESTAMPTZ NOT NULL,
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- "the current ride of a user" is the latest one
CREATE INDEX ride_sessions_user_idx ON ride_sessions (user_id, updated_at DESC);
