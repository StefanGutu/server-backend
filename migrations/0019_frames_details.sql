-- What the camera said about a photo, kept so a photo can be described later (display info: the list of photos of
-- a bus). Rows stored before this migration have none of these: NULL.

ALTER TABLE frames
    ADD COLUMN route_id INTEGER,
    ADD COLUMN width    INTEGER,
    ADD COLUMN height   INTEGER;
