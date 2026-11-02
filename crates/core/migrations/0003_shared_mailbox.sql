-- Shared mailboxes (Microsoft 365): the user who signs in differs from the mailbox address.
-- NULL for regular accounts.
ALTER TABLE accounts ADD COLUMN auth_user TEXT;
