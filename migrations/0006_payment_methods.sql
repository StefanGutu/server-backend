-- Cards of a user (server-api.md section 4). Card data never reaches the server:
-- only the payment provider's id of the card and the display data the app sends.

CREATE TABLE payment_methods (
    -- time-ordered uuid (v7): sorting by id lists the cards in the order they were added
    id                 uuid PRIMARY KEY,
    -- a deleted user takes their cards with them
    user_id            uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    -- the provider's id of the card ("paymentMethodId" in the app's request)
    provider_method_id TEXT NOT NULL,
    network            TEXT NOT NULL CHECK (network IN ('visa', 'mastercard', 'other')),
    last4              TEXT NOT NULL CHECK (last4 ~ '^[0-9]{4}$'),
    holder             TEXT NOT NULL,
    -- "MM/YY"
    expiry             TEXT NOT NULL CHECK (expiry ~ '^(0[1-9]|1[0-2])/[0-9]{2}$'),
    bank               TEXT,
    tag                TEXT CHECK (tag IN ('Google Pay', 'Apple Pay')),
    -- adding the same card twice gives the same row
    UNIQUE (user_id, provider_method_id)
);
