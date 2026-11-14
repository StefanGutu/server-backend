-- Reports of problems on a line (server-api.md: Report, GET /reports, POST /reports, POST /reports/{id}/confirm).

CREATE TABLE reports (
    -- time-ordered uuid (v7)
    id         uuid PRIMARY KEY,
    kind       TEXT NOT NULL CHECK (kind IN ('traffic', 'accident', 'breakdown', 'works', 'crowding', 'detour')),
    -- the name of the line (the route name, such as "E2")
    line       TEXT NOT NULL,
    -- the stop nearest to the bus of that line the reporter was on
    stop_name  TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- a report is shown until this time
    expires_at TIMESTAMPTZ NOT NULL
);

-- GET /reports lists what has not expired yet
CREATE INDEX reports_expires_idx ON reports (expires_at);

-- Who says the report is true. The reporter is the first row. A user counts once.
CREATE TABLE report_confirmations (
    report_id uuid NOT NULL REFERENCES reports (id) ON DELETE CASCADE,
    -- a deleted user takes their confirmations with them
    user_id   uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    PRIMARY KEY (report_id, user_id)
);
