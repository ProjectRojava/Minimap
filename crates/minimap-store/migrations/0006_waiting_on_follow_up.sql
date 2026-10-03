-- Snooze: a waiting-on with a future follow_up_on is hidden until that date.
ALTER TABLE waiting_on ADD COLUMN follow_up_on TEXT;
