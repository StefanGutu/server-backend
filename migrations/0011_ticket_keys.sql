-- The keys that sign the QR tickets (EdDSA / Ed25519, server-api.md section 5).
-- A key is created at the first start. A new key can be added later: GET /tickets/public-key
-- lists all of them, and a token says which one signed it in its `kid` header.

CREATE TABLE ticket_keys (
    -- time-ordered uuid (v7) as text: the largest kid is the newest key, the one that signs
    kid         TEXT PRIMARY KEY,
    -- the 32 bytes of the public key, base64url without padding (what the app gets)
    public_key  TEXT  NOT NULL,
    -- the private key as PKCS#8 DER bytes. Stored as is, not encrypted: see doc/reminders.md R24
    private_key BYTEA NOT NULL
);
