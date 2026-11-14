-- A user has at most one open ride. Two sightings for two buses sent at the same moment
-- could each start a `pending` ride; the database now allows only one.
-- Open = pending, awaiting_confirmation, payment_failed, confirmed.

-- Old data: if a user has several open rides, keep the newest and reject the others,
-- otherwise the index below cannot be created.
UPDATE ride_sessions SET status = 'rejected', version = version + 1, updated_at = now()
WHERE status IN ('pending', 'awaiting_confirmation', 'payment_failed', 'confirmed')
  AND id NOT IN (
      SELECT DISTINCT ON (user_id) id FROM ride_sessions
      WHERE status IN ('pending', 'awaiting_confirmation', 'payment_failed', 'confirmed')
      ORDER BY user_id, updated_at DESC, id DESC
  );

CREATE UNIQUE INDEX ride_sessions_one_open_per_user ON ride_sessions (user_id)
WHERE status IN ('pending', 'awaiting_confirmation', 'payment_failed', 'confirmed');
