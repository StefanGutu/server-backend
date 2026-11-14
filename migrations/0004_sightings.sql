-- "This user heard this bus in this 30-second slot" (server-api.md section 1).

CREATE TABLE sightings (
    -- a deleted user takes the sightings with them (DELETE /me must keep working)
    user_id     uuid    NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    bus_id      INTEGER NOT NULL REFERENCES buses (bus_id),
    -- unix time / 30
    slot        BIGINT  NOT NULL,
    -- raw dBm, a hint only
    rssi        INTEGER NOT NULL,
    received_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- one per user, bus and slot: the app may send the same one again after a network error
    PRIMARY KEY (user_id, bus_id, slot)
);
