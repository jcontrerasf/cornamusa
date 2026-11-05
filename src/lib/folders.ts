import type { FolderRole } from './api';
import { Inbox, Send, FileText, Archive, ShieldAlert, Trash2, Folder, Star, Layers } from '@lucide/svelte';

/** Sidebar icon for each folder role (shared by the sidebar and the settings list). */
export const FOLDER_ICONS: Record<FolderRole, typeof Inbox> = {
  inbox: Inbox, sent: Send, drafts: FileText, archive: Archive, junk: ShieldAlert,
  trash: Trash2, all: Layers, flagged: Star, other: Folder,
};

/** Roles whose unread count is worth showing (not sent, drafts, trash…). */
export function showsUnread(role: FolderRole): boolean {
  return role === 'inbox' || role === 'junk' || role === 'other';
}
