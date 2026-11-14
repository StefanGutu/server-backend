-- Transport passes of a user (server-api.md section 4, "abonament").

CREATE TABLE passes (
    -- time-ordered uuid (v7), like the cards
    id          uuid PRIMARY KEY,
    -- a deleted user takes their passes with them
    user_id     uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    name        TEXT NOT NULL,
    holder      TEXT NOT NULL,
    valid_from  DATE NOT NULL,
    valid_until DATE NOT NULL CHECK (valid_until >= valid_from),
    -- "all" or ["15", "33"]
    lines       JSONB NOT NULL,
    theme       TEXT NOT NULL CHECK (theme IN ('ocean', 'sunset', 'forest')),
    -- the number of the physical pass card
    card_number TEXT NOT NULL,
    -- no verification service exists yet: always false for now
    verified    BOOLEAN NOT NULL DEFAULT false,
    -- adding the same pass card twice gives the same row
    UNIQUE (user_id, card_number)
);
