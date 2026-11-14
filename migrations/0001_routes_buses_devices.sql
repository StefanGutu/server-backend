-- Lines, buses and the two ESP32 devices on each bus (server.md).

CREATE TABLE routes (
    route_id INTEGER PRIMARY KEY CHECK (route_id BETWEEN 0 AND 65535),
    name     TEXT    NOT NULL,
    -- ordered list of { lat, lon, name? }; a point with a name is a stop
    points   JSONB   NOT NULL
);

CREATE TABLE buses (
    bus_id   INTEGER PRIMARY KEY CHECK (bus_id BETWEEN 0 AND 65535),
    route_id INTEGER NOT NULL REFERENCES routes (route_id)
);

CREATE TABLE devices (
    device_id TEXT PRIMARY KEY CHECK (device_id ~ '^[0-9a-f]{12}$'),
    bus_id    INTEGER NOT NULL REFERENCES buses (bus_id),
    role      TEXT    NOT NULL CHECK (role IN ('beacon', 'camera')),
    -- used as the HMAC key as-is (ASCII), never hex-decoded
    secret    TEXT    NOT NULL
);

-- Seed: bus 101 on route 7 (line E2), example route from server.md.
INSERT INTO routes (route_id, name, points) VALUES
    (7, 'E2', '[
        {"lat": 45.7500, "lon": 21.2200, "name": "Central Station"},
        {"lat": 45.7500, "lon": 21.2260},
        {"lat": 45.7530, "lon": 21.2290, "name": "Market Square"}
    ]');

INSERT INTO buses (bus_id, route_id) VALUES (101, 7);

-- DEMO secrets only. Replace with the real ones from env/node_a/.env and env/node_b/.env:
--   UPDATE devices SET device_id = '<real id>', secret = '<real secret>' WHERE role = 'beacon';
INSERT INTO devices (device_id, bus_id, role, secret) VALUES
    ('a4cf12e08b10', 101, 'beacon',
     '0000000000000000000000000000000000000000000000000000000000000001'),
    ('a4cf12e08b11', 101, 'camera',
     '0000000000000000000000000000000000000000000000000000000000000002');
