-- What the display calls need to show about users, sightings, rides, charges, passes, reports and ticket checks
-- (server-api display info). Everything here is **recorded**; nothing changes how a call works.

-- ---- users: when the user was last seen (any call with their login, updated at most once a minute) ----
ALTER TABLE users ADD COLUMN last_seen_at TIMESTAMPTZ;

-- ---- logins: why a refresh token stopped working ----
-- rotated = used for a refresh (normal), logout, token_reuse = a used token came back (revoked the device)
ALTER TABLE refresh_tokens ADD COLUMN revoked_reason TEXT
    CHECK (revoked_reason IN ('rotated', 'logout', 'token_reuse'));
UPDATE refresh_tokens SET revoked_reason = 'rotated' WHERE revoked_at IS NOT NULL;

-- ---- passes: when a pass was last renewed ----
ALTER TABLE passes ADD COLUMN last_renewed_at TIMESTAMPTZ;

-- ---- charges: which pass product a renewal bought ----
ALTER TABLE charges ADD COLUMN pass_product_id uuid;

-- ---- reports: where, which bus and who ----
ALTER TABLE reports
    ADD COLUMN bus_id           INTEGER,
    ADD COLUMN stop_lat         DOUBLE PRECISION,
    ADD COLUMN stop_lon         DOUBLE PRECISION,
    ADD COLUMN reporter_user_id uuid REFERENCES users (id) ON DELETE SET NULL;

-- ---- every sighting that reached the server, the accepted and the refused ----
-- (`sightings` keeps only the accepted ones, for the boarding decision)
CREATE TABLE sighting_log (
    id            BIGSERIAL PRIMARY KEY,
    received_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    user_id       uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    bus_id        INTEGER NOT NULL,
    slot          BIGINT NOT NULL,
    -- the slot minus the slot of the server clock when it arrived
    slot_offset   BIGINT NOT NULL,
    rssi          INTEGER NOT NULL,
    clock_synced  BOOLEAN NOT NULL,
    accepted      BOOLEAN NOT NULL,
    -- bad_token, slot_too_old, slot_ahead, clock_not_synced, unknown_bus, no_beacon
    reject_reason TEXT
);
CREATE INDEX sighting_log_id_idx ON sighting_log (id DESC);

-- ---- rides: when, how it ended, how it started, what qualified it ----
ALTER TABLE ride_sessions
    ADD COLUMN created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    ADD COLUMN confirmed_at  TIMESTAMPTZ,
    ADD COLUMN ended_at      TIMESTAMPTZ,
    -- declined, timeout, not_boarded, silence_90s, silence_60min
    ADD COLUMN end_reason    TEXT,
    -- the user said "I am on the bus"
    ADD COLUMN manual        BOOLEAN NOT NULL DEFAULT false,
    -- what made the server offer the ride: {"slots_in_row": 3, "distance_m": 412}; NULL for a manual ride
    ADD COLUMN qualification JSONB;
UPDATE ride_sessions SET created_at = updated_at;

CREATE TABLE ride_events (
    id         BIGSERIAL PRIMARY KEY,
    session_id uuid NOT NULL REFERENCES ride_sessions (id) ON DELETE CASCADE,
    status     TEXT NOT NULL,
    at         TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX ride_events_session_idx ON ride_events (session_id, id);
INSERT INTO ride_events (session_id, status, at) SELECT id, status, updated_at FROM ride_sessions;

-- A trigger keeps the history and the times, so no call that changes a ride can forget them.
CREATE FUNCTION ride_sessions_times() RETURNS trigger AS $$
BEGIN
    IF NEW.status IS DISTINCT FROM OLD.status THEN
        IF NEW.status = 'confirmed' THEN NEW.confirmed_at := now(); END IF;
        IF NEW.status IN ('ended', 'rejected') THEN NEW.ended_at := now(); END IF;
    END IF;
    RETURN NEW;
END $$ LANGUAGE plpgsql;
CREATE TRIGGER ride_sessions_times BEFORE UPDATE ON ride_sessions
    FOR EACH ROW EXECUTE FUNCTION ride_sessions_times();

CREATE FUNCTION ride_sessions_history() RETURNS trigger AS $$
BEGIN
    IF TG_OP = 'INSERT' OR NEW.status IS DISTINCT FROM OLD.status THEN
        INSERT INTO ride_events (session_id, status) VALUES (NEW.id, NEW.status);
    END IF;
    RETURN NULL;
END $$ LANGUAGE plpgsql;
CREATE TRIGGER ride_sessions_history AFTER INSERT OR UPDATE ON ride_sessions
    FOR EACH ROW EXECUTE FUNCTION ride_sessions_history();

-- ---- what the inspectors scanned ----
CREATE TABLE ticket_verifications (
    id           BIGSERIAL PRIMARY KEY,
    at           TIMESTAMPTZ NOT NULL DEFAULT now(),
    inspector_id uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    -- NULL when the text was not a ticket of ours
    charge_id    uuid,
    valid        BOOLEAN NOT NULL,
    paid         BOOLEAN NOT NULL,
    line         TEXT,
    passengers   INTEGER,
    expires_at   TIMESTAMPTZ
);
CREATE INDEX ticket_verifications_id_idx ON ticket_verifications (id DESC);
