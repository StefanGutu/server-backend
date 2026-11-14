-- GET /charges for the display can ask for a time range of all users (doc/PLAN.md, step 43): this index lets it find
-- the range without reading the whole table. The index of one user's charges (migration 0010) cannot do that.
CREATE INDEX charges_started_idx ON charges (started_at DESC);
