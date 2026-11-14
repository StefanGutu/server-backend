-- What a user was charged for a ride or a pass (server-api.md: Charge).

CREATE TABLE charges (
    id          uuid PRIMARY KEY,
    -- a deleted user takes their charges with them (see doc/reminders.md R14)
    user_id     uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    -- the ride this charge is for; none for the purchase of a pass (renew, step 23)
    session_id  uuid REFERENCES ride_sessions (id) ON DELETE SET NULL,
    line        TEXT NOT NULL,
    destination TEXT,
    -- money is kept in bani (500 = 5 lei), the app gets lei
    amount_bani INTEGER NOT NULL CHECK (amount_bani >= 0),
    status      TEXT NOT NULL CHECK (status IN ('in_progress', 'completed', 'covered_by_pass', 'cancelled')),
    -- id of the card or the pass that paid (they live in two tables, so no foreign key)
    paid_with   uuid NOT NULL,
    passengers  INTEGER NOT NULL DEFAULT 1 CHECK (passengers BETWEEN 1 AND 10),
    started_at  TIMESTAMPTZ NOT NULL,
    ended_at    TIMESTAMPTZ,
    qr_token    TEXT NOT NULL
);

-- GET /charges lists the newest first, one user at a time
CREATE INDEX charges_user_idx ON charges (user_id, started_at DESC, id DESC);
