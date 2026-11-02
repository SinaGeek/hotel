-- A3: per-night rates in the mirror — room revenue uses the server's actual
-- priced rates (JSON array of {date, price_cents}) instead of an even split.
-- Older mirrors (user_version 0/1) gain the column; fresh ones create with it
-- via 0001 + this ALTER guarded by open()'s user_version check.
ALTER TABLE reservations ADD COLUMN daily_rates TEXT;
