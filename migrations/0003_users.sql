-- App users (server-api.md section 3).

CREATE TABLE users (
    id                uuid PRIMARY KEY,
    -- anonymous: only device login, cannot pay. account: linked to a phone/google/apple identity
    tier              TEXT NOT NULL CHECK (tier IN ('anonymous', 'account')),
    -- inspector can verify tickets (step 22)
    role              TEXT NOT NULL DEFAULT 'user' CHECK (role IN ('user', 'inspector')),
    name              TEXT,
    email             TEXT,
    -- autoCharge, tripReminders, arrivalAlerts, disruptions, locationSharing
    settings          JSONB NOT NULL,
    -- id of a card or pass in the wallet (no foreign key: they live in two tables)
    default_source_id uuid,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now()
);
