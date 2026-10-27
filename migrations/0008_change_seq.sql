-- 0008: change-seq allocation without row contention (README §13 load targets).
--
-- record_change used to bump hotels.next_change_seq (row-locked until commit),
-- serializing every concurrent write in a hotel: under 24 concurrent saves the
-- commit lock convoy pushed p99 past 2 s. A dedocated PG sequence allocates
-- monotonically increasing values with no row lock (gaps are allowed — the
-- sync pull cursor only needs per-hotel increasing order).
--
-- change_log change_seq allocation (migration 0008) uses a dedicated PG
-- sequence: monotonically increasing per hotel, allocated with no row lock.
-- hotels.next_change_seq remains for display compatibility and is served live
-- from change_log.max(hotel) by the hotels endpoints.
DO $$
DECLARE
  start_at bigint;
BEGIN
  SELECT COALESCE(max(next_change_seq), 0) + 1 INTO start_at FROM hotels;
  EXECUTE format('CREATE SEQUENCE IF NOT EXISTS change_log_seq START WITH %s', start_at);
END $$;
