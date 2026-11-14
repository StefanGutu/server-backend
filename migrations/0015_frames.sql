-- Cabin photos from Node B (server.md: POST /buses/{bus_id}/frames). The JPEG is a file on disk,
-- the table keeps where it is and what the camera said about it. Photos are only stored: nothing
-- counts the people in them.

CREATE TABLE frames (
    -- where the file is (relative to the folder the server runs in); every photo has its own file
    file_path   TEXT PRIMARY KEY,
    bus_id      INTEGER NOT NULL REFERENCES buses (bus_id),
    -- the camera's counter; it starts again from 0 when the camera restarts, so it is not unique
    frame_seq   BIGINT  NOT NULL CHECK (frame_seq >= 0),
    -- unix seconds when the photo was taken, 0 if the camera had no clock yet
    capture_ts  BIGINT  NOT NULL CHECK (capture_ts >= 0),
    received_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- "the latest photos of one bus"
CREATE INDEX frames_bus_received_idx ON frames (bus_id, received_at DESC);
