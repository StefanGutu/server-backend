-- Login with a phone number (server-api.md section 3: /auth/otp/start, /auth/link, /auth/signin).

-- The code sent to a phone. One code per phone: asking for a new one replaces the old one.
CREATE TABLE otp_codes (
    -- "+" and the digits, as the server writes them (see src/modules/auth/otp.rs)
    phone      TEXT PRIMARY KEY,
    -- sha256 of the phone and the code. The code itself is never stored
    code_hash  TEXT NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    -- how many times a code was tried for this phone; too many and the code is dead
    attempts   INTEGER NOT NULL DEFAULT 0
);

-- Who a user is outside the app: a phone number (later a Google or an Apple account).
CREATE TABLE auth_identities (
    -- a deleted user takes the identities with them
    user_id  uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    provider TEXT NOT NULL CHECK (provider IN ('google', 'apple', 'phone')),
    -- the phone number, or the id of the Google / Apple account
    subject  TEXT NOT NULL,
    -- one identity belongs to one user (IDENTITY_IN_USE for a second one)
    PRIMARY KEY (provider, subject)
);

CREATE INDEX auth_identities_user ON auth_identities (user_id);
