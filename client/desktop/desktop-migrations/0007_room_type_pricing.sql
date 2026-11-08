-- Room type pricing and per-room identity (migration 0007, applied by the
-- Rust runner with guarded ALTERs; see local_store.rs init_connection).
--
-- Fee model layers (lowest wins when nothing above matches):
--   1. room_types.base_price_cents     - normal / midweek nightly fee
--   2. room_types.weekend_price_cents  - weekend nightly fee (NULL/0 = same as mid)
--   3. price_rules                     - calendar overrides: one row per
--      date range; date_from == date_to is a single-day price, spanning
--      ranges act as batch changes for that period.
ALTER TABLE room_types ADD COLUMN weekend_price_cents INTEGER;
ALTER TABLE rooms      ADD COLUMN name TEXT;
ALTER TABLE price_rules ADD COLUMN hotel_id INTEGER NOT NULL DEFAULT 4442;
