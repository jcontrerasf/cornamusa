-- Highest UID ever seen per folder. Kept separately from the cached messages so an emptied
-- folder ("inbox zero") still knows it was synced and notifies about the next message.
ALTER TABLE folders ADD COLUMN last_uid INTEGER;
