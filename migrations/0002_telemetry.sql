-- GPS positions from Node A (server.md). One row per (device, boot, seq).

CREATE TABLE telemetry (
    id          BIGSERIAL PRIMARY KEY,
    bus_id      INTEGER NOT NULL REFERENCES buses (bus_id),
    device_id   TEXT    NOT NULL REFERENCES devices (device_id),
    boot_id     TEXT    NOT NULL,
    seq         BIGINT  NOT NULL,
    -- unix seconds, 0 when the device clock was not set yet
    ts          BIGINT  NOT NULL,
    uptime_ms   BIGINT  NOT NULL,
    -- lat/lon are meaningless when fix_ok is false
    lat         DOUBLE PRECISION,
    lon         DOUBLE PRECISION,
    speed_kmh   DOUBLE PRECISION,
    fix_ok      BOOLEAN NOT NULL,
    -- 1 = simulator, 0 = real GPS (never charge on 1)
    sim         SMALLINT NOT NULL CHECK (sim IN (0, 1)),
    received_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- the retry rule: same message sent twice is stored once
    UNIQUE (device_id, boot_id, seq)
);

-- Later steps read the latest positions of one bus.
CREATE INDEX telemetry_bus_received_idx ON telemetry (bus_id, received_at DESC);
