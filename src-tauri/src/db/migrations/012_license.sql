-- Windows license: the activated key and the last time the app ran (to spot a clock turned back).
ALTER TABLE app_settings ADD COLUMN license_key TEXT;
ALTER TABLE app_settings ADD COLUMN license_activated_at TEXT;
ALTER TABLE app_settings ADD COLUMN last_seen_at TEXT;
