-- What the model that counts people (site/headcount) saw in a photo (doc/PLAN.md, step 38).
-- NULL = the photo was not counted. Only the newest photo of a bus is ever sent to the model, so most old photos stay NULL.
ALTER TABLE frames
    ADD COLUMN people_count INTEGER CHECK (people_count >= 0),
    ADD COLUMN counted_at   TIMESTAMPTZ;
