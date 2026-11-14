-- The passes a user can buy (server-api.md: GET /passes/catalog, and `productId` of the renew call).

CREATE TABLE pass_products (
    id         uuid PRIMARY KEY,
    name       TEXT NOT NULL,
    -- how many days the pass is valid
    days       INTEGER NOT NULL CHECK (days > 0),
    -- money is kept in bani (1000 = 10 lei), the app gets lei
    price_bani INTEGER NOT NULL CHECK (price_bani >= 0),
    -- "all" or ["E2", "7"]
    lines      JSONB NOT NULL
);

-- PLACEHOLDER products: names, days, prices and lines are examples, not the real offer.
-- Change them with a new migration (or UPDATE) when the real catalog is known.
INSERT INTO pass_products (id, name, days, price_bani, lines) VALUES
    ('a0000000-0000-4000-8000-000000000001', 'Day pass',         1,  1000, '"all"'),
    ('a0000000-0000-4000-8000-000000000002', 'Weekly pass',      7,  4000, '"all"'),
    ('a0000000-0000-4000-8000-000000000003', 'Monthly pass',    30, 12000, '"all"'),
    ('a0000000-0000-4000-8000-000000000004', 'Monthly pass E2', 30,  6000, '["E2"]');
