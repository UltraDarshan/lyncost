-- Windows trial: the timestamp is set once, on first read, by get_trial_status().
-- Stored in the database (not localStorage) so clearing browser/app cache alone
-- cannot silently restart the trial.
ALTER TABLE app_settings ADD COLUMN trial_started_at TEXT;
