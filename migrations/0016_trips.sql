-- The trips of a user (server-api.md: RecentTrip, SavedRoute). A trip is written when a ride ends.

CREATE TABLE trips (
    id          uuid PRIMARY KEY,
    -- a deleted user takes the trips with them
    user_id     uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    -- the charge of the ride: one trip for each charge
    charge_id   uuid NOT NULL UNIQUE REFERENCES charges (id) ON DELETE CASCADE,
    -- the stops (names from the route) nearest to where the bus was when the user got on and off
    from_stop   TEXT NOT NULL,
    to_stop     TEXT NOT NULL,
    -- when the user got on, in UTC
    date        DATE NOT NULL,
    time        TIME NOT NULL,
    line        TEXT NOT NULL,
    -- money is kept in bani, the app gets lei
    fare_bani   INTEGER NOT NULL CHECK (fare_bani >= 0),
    distance_km DOUBLE PRECISION NOT NULL CHECK (distance_km >= 0),
    duration_min INTEGER NOT NULL CHECK (duration_min >= 0)
);

-- GET /trips/recent lists the newest first, one user at a time (the ids are time-ordered)
CREATE INDEX trips_user_idx ON trips (user_id, id DESC);

-- Routes a user keeps. Nothing creates them yet (server-api.md has no call for it).
CREATE TABLE saved_routes (
    id        uuid PRIMARY KEY,
    user_id   uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    from_stop TEXT NOT NULL,
    to_stop   TEXT NOT NULL,
    line      TEXT NOT NULL
);

CREATE INDEX saved_routes_user_idx ON saved_routes (user_id);
