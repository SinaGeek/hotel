-- 0011: Add accountant role and allow multiple comma-separated roles per user

INSERT INTO roles (name, is_system, description) VALUES
  ('accountant', TRUE, 'Financial and accounting staff')
ON CONFLICT (name) DO NOTHING;

INSERT INTO role_permissions (role, permission, allowed) VALUES
  ('accountant', 'financial_view', TRUE),
  ('accountant', 'payment_record', TRUE),
  ('accountant', 'payment_refund', TRUE),
  ('accountant', 'folio_manage', TRUE),
  ('accountant', 'report_view_tenant', TRUE),
  ('accountant', 'reservation_view', TRUE),
  ('accountant', 'guest_export', TRUE)
ON CONFLICT (role, permission) DO NOTHING;

-- Drop foreign key so users.role can store multiple roles (e.g. 'reception,accountant')
ALTER TABLE users DROP CONSTRAINT IF EXISTS users_role_fkey;
