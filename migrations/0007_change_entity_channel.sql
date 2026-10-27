-- 0007: extend change_log entity types with 'channel' (per-hotel configurable channels)
ALTER TABLE change_log DROP CONSTRAINT change_log_entity_type_check;
ALTER TABLE change_log ADD CONSTRAINT change_log_entity_type_check
  CHECK (entity_type IN
    ('room_type','room','room_status','reservation','guest','payment',
     'folio_transaction','price_rule','user','channel','hotel'));
