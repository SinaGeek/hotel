-- Add user sina (password: 123) to hotel 4442
-- Hash generated with: argon2 '123' -e (Argon2id, default params)
INSERT INTO users (id, hotel_id, username, password_hash, role, active, force_password_change)
VALUES (
  gen_random_uuid(),
  4442,
  'sina',
  '$argon2id$v=19$m=19456,t=2,p=1$8PtGZ3sJT8KMkRhYWvdXhQ$L2JXXP+L9qEQqmUXHpUQrNlGmN4lHMJoVGpWY7VUTRU',
  'manager',
  true,
  false
)
ON CONFLICT (hotel_id, username) DO UPDATE
SET password_hash = EXCLUDED.password_hash,
    active = true,
    force_password_change = false;
