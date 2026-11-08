-- 0006: system roles + default permission matrix (README §6)
-- Permissions are DATA. The server reads the matrix from these tables; the Rust
-- `roles.rs` matrix becomes a fallback reference and its reception-report rule
-- is corrected: ReportViewTenant defaults OFF for reception, configurable
-- per hotel via hotel_permission_overrides.

INSERT INTO roles (name, is_system, description) VALUES
  ('admin',        TRUE, 'Platform admin (hotel 0)'),
  ('manager',      TRUE, 'Hotel manager'),
  ('reception',    TRUE, 'Front desk'),
  ('housekeeping', TRUE, 'Housekeeping staff')
ON CONFLICT (name) DO NOTHING;

INSERT INTO role_permissions (role, permission, allowed) VALUES
  -- admin: everything (all hotels)
  ('admin','hotel_create', TRUE),('admin','hotel_edit', TRUE),('admin','hotel_disable', TRUE),
  ('admin','hotel_view_all', TRUE),
  ('admin','user_manage', TRUE),('admin','user_view', TRUE),
  ('admin','room_type_manage', TRUE),('admin','room_manage', TRUE),('admin','price_rule_manage', TRUE),
  ('admin','reservation_view', TRUE),('admin','reservation_create', TRUE),
  ('admin','reservation_edit', TRUE),('admin','reservation_check_in', TRUE),
  ('admin','reservation_check_out', TRUE),('admin','reservation_cancel', TRUE),
  ('admin','financial_view', TRUE),('admin','payment_record', TRUE),
  ('admin','payment_refund', TRUE),('admin','folio_manage', TRUE),
  ('admin','housekeeping_view', TRUE),('admin','housekeeping_update_status', TRUE),
  ('admin','report_view_tenant', TRUE),('admin','report_view_platform', TRUE),
  ('admin','audit_log_view', TRUE),

  -- manager: own hotel, users below manager level
  ('manager','user_manage', TRUE),('manager','user_view', TRUE),
  ('manager','room_type_manage', TRUE),('manager','room_manage', TRUE),('manager','price_rule_manage', TRUE),
  ('manager','reservation_view', TRUE),('manager','reservation_create', TRUE),
  ('manager','reservation_edit', TRUE),('manager','reservation_check_in', TRUE),
  ('manager','reservation_check_out', TRUE),('manager','reservation_cancel', TRUE),
  ('manager','financial_view', TRUE),('manager','payment_record', TRUE),
  ('manager','payment_refund', TRUE),('manager','folio_manage', TRUE),
  ('manager','housekeeping_view', TRUE),('manager','housekeeping_update_status', TRUE),
  ('manager','report_view_tenant', TRUE),('manager','audit_log_view', TRUE),

  -- reception: reservations + payments + housekeeping; reports DEFAULT OFF (README §6)
  ('reception','user_view', TRUE),
  ('reception','reservation_view', TRUE),('reception','reservation_create', TRUE),
  ('reception','reservation_edit', TRUE),('reception','reservation_check_in', TRUE),
  ('reception','reservation_check_out', TRUE),('reception','reservation_cancel', TRUE),
  ('reception','financial_view', TRUE),('reception','payment_record', TRUE),
  ('reception','payment_refund', TRUE),('reception','folio_manage', TRUE),
  ('reception','housekeeping_view', TRUE),('reception','housekeeping_update_status', TRUE),
  ('reception','report_view_tenant', FALSE),

  -- housekeeping: room board + cleanliness only; no guest names, prices, payments
  ('housekeeping','housekeeping_view', TRUE),
  ('housekeeping','housekeeping_update_status', TRUE)
ON CONFLICT (role, permission) DO NOTHING;
