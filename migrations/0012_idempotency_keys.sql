-- Answers to calls that carry an `Idempotency-Key` (confirm a ride, renew a pass: server-api.md).
-- The same key sent again gets the stored answer and the operation is not done a second time.

CREATE TABLE idempotency_keys (
    -- the uuid the app put in the header
    key          uuid NOT NULL,
    -- a deleted user takes the stored answers with them
    user_id      uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    -- which call it was, for example "POST /rides/<id>/confirm"
    endpoint     TEXT NOT NULL,
    -- sha256 of the call and its body: the same key with another body is a conflict
    request_hash TEXT NOT NULL,
    -- the answer of the first call (only successful ones are stored)
    response     JSONB NOT NULL,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- a key belongs to one user: two users may send the same uuid
    PRIMARY KEY (user_id, key)
);
