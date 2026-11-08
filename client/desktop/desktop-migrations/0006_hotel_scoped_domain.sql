-- Make core hotel domain records explicitly hotel-scoped for standalone desktop.
ALTER TABLE guests ADD COLUMN hotel_id INTEGER NOT NULL DEFAULT 4442;
ALTER TABLE room_types ADD COLUMN hotel_id INTEGER NOT NULL DEFAULT 4442;
ALTER TABLE rooms ADD COLUMN hotel_id INTEGER NOT NULL DEFAULT 4442;
ALTER TABLE channels ADD COLUMN hotel_id INTEGER NOT NULL DEFAULT 4442;
ALTER TABLE reservations ADD COLUMN hotel_id INTEGER NOT NULL DEFAULT 4442;
