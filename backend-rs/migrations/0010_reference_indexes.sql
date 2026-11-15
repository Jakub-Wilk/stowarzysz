-- Lookups by a referencing column that had no index, so each one scanned its table.
-- Deleting a push subscription (gone, or failing for good) cascades to its queued notifications.
create index push_outbox_subscription on push_outbox (subscription_id);
-- The media sweeper checks whether a trashed key is still a poll's picture.
create index polls_proposed_image on polls (proposed_image_key) where proposed_image_key is not null;
create index polls_previous_image on polls (previous_image_key) where previous_image_key is not null;
