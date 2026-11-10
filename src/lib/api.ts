// Typed wrappers over the Tauri commands. Types mirror crates/core/src/model.rs.
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

export type Provider = 'gmail' | 'outlook' | 'imap';
export type Security = 'tls' | 'starttls';
export type FolderRole = 'inbox' | 'sent' | 'drafts' | 'junk' | 'trash' | 'archive' | 'all' | 'flagged' | 'other';
export type MoveTarget = 'inbox' | 'archive' | 'junk' | 'trash';

export interface ServerConfig { host: string; port: number; security: Security }

export interface Account {
  id: string;
  provider: Provider;
  email: string;
  displayName: string;
  username: string;
  imap: ServerConfig;
  smtp: ServerConfig;
  color: string;
  /** Shared mailbox: the user who signs in (and sends); null for regular accounts. */
  authUser: string | null;
}

export interface Folder {
  id: number;
  accountId: string;
  path: string;
  name: string;
  role: FolderRole;
  unread: number;
  total: number;
}

export interface EmailAddress { name: string | null; email: string }

export interface MessageSummary {
  id: number;
  accountId: string;
  folderId: number;
  uid: number;
  messageId: string | null;
  threadId: string;
  subject: string;
  from: EmailAddress | null;
  to: EmailAddress[];
  cc: EmailAddress[];
  date: number;
  preview: string;
  seen: boolean;
  flagged: boolean;
  answered: boolean;
  hasAttachments: boolean;
}

export type AttachmentKind = 'image' | 'pdf' | 'external' | 'saveOnly';
export interface AttachmentInfo {
  index: number; filename: string; mimeType: string; size: number; inline: boolean;
  /** What the UI may do: view (image/pdf), open with a system app, or only save. */
  kind: AttachmentKind;
  /** Whether it may be handed to a system application. */
  canOpen: boolean;
}

export interface MessageBody {
  summary: MessageSummary;
  replyTo: EmailAddress[];
  inReplyTo: string | null;
  references: string[];
  html: string | null;
  text: string | null;
  remoteContentBlocked: boolean;
  attachments: AttachmentInfo[];
  chatText: string;
  /** Strictly sanitized HTML of the new part (no style/class/id); null when the mail has no HTML. */
  chatHtml: string | null;
}

export interface Thread {
  threadId: string;
  subject: string;
  participants: EmailAddress[];
  recipients: EmailAddress[];
  lastDate: number;
  messageCount: number;
  unreadCount: number;
  flagged: boolean;
  hasAttachments: boolean;
  preview: string;
  latestId: number;
  accountId: string;
  pinned: boolean;
}

export interface Signature {
  id: string;
  accountId: string | null;
  name: string;
  isHtml: boolean;
  body: string;
  isDefault: boolean;
}

export interface OutgoingMessage {
  accountId: string;
  to: EmailAddress[];
  cc: EmailAddress[];
  bcc: EmailAddress[];
  subject: string;
  html: string;
  text: string;
  attachments: { path: string }[];
  inReplyTo: string | null;
  references: string[];
  forwardAttachmentsOf: number | null;
}

export interface Settings {
  closeToTray: boolean;
  notifications: boolean;
  doNotDisturb: boolean;
  chatView: boolean;
  theme: 'system' | 'light' | 'dark';
  loadRemoteImages: boolean;
  startMinimized: boolean;
  sidebarCollapsed: boolean;
  /** Bundled sound name, 'custom', 'system' or 'none'. */
  newMailSound: string;
  /** Folders hidden from the sidebar, as "accountId:path". */
  hiddenFolders: string[];
  /** Original name of the user's own sound file (copied into the app's data folder). */
  customSoundName: string | null;
  /** Conversations kept open as tabs, in order. */
  openTabs: OpenTab[];
  /** Key that sends the chat quick reply. */
  chatSendKey: SendKey;
  /** Key that sends from the full composer ('enter' is not offered there). */
  composerSendKey: SendKey;
  /** Default signatures per account id; accounts without an entry use the signatures' isDefault. */
  signatureDefaults: Record<string, SignatureDefaults>;
  /** New-mail sound per account id, overriding newMailSound. */
  accountSounds: Record<string, string>;
  /** Lets Claude read mail and open drafts through MCP. */
  mcpEnabled: boolean;
}

/** Draft prepared by Claude through MCP (see crates/core/src/mcp.rs). */
export interface DraftRequest {
  accountId: string | null;
  replyTo: { accountId: string; threadId: string } | null;
  replyAll: boolean;
  to: EmailAddress[];
  cc: EmailAddress[];
  subject: string | null;
  /** Plain text, without signature or quote. */
  body: string;
}

export type SendKey = 'enter' | 'ctrlEnter' | 'none';

/** Signature ids; null means no signature. */
export interface SignatureDefaults { new: string | null; reply: string | null }

export interface OpenTab { accountId: string; threadId: string; subject: string }

export interface SettingsView extends Settings { autostart: boolean }

export interface DetectedConfig {
  provider: Provider;
  imap: ServerConfig;
  smtp: ServerConfig;
  appPasswordHint: string | null;
}

export interface PickedFile { path: string; name: string; size: number }

export interface NewMailEvent { accountId: string; folderId: number; messages: MessageSummary[] }

export type MailEvent =
  | { type: 'accountsChanged' }
  | { type: 'mailboxChanged'; accountId: string }
  | ({ type: 'newMail' } & NewMailEvent)
  | { type: 'syncStatus'; accountId: string; syncing: boolean; error: string | null }
  | { type: 'unreadCount'; count: number };

export const api = {
  listAccounts: () => invoke<Account[]>('list_accounts'),
  detectConfig: (email: string) => invoke<DetectedConfig>('detect_config', { email }),
  addOAuthAccount: (provider: Provider, loginHint: string | null) =>
    invoke<Account>('add_oauth_account', { provider, loginHint }),
  addSharedMailbox: (sharedEmail: string, loginHint: string | null) =>
    invoke<Account>('add_shared_mailbox', { sharedEmail, loginHint }),
  addPasswordAccount: (request: {
    email: string; displayName: string; username: string | null; password: string; imap: ServerConfig; smtp: ServerConfig;
  }) => invoke<Account>('add_password_account', { request }),
  removeAccount: (id: string) => invoke<void>('remove_account', { id }),
  updateAccount: (id: string, displayName: string, color: string) =>
    invoke<void>('update_account', { id, displayName, color }),

  listFolders: (accountId: string) => invoke<Folder[]>('list_folders', { accountId }),
  syncAccount: (accountId: string) => invoke<void>('sync_account', { accountId }),
  syncFolder: (folderId: number) => invoke<void>('sync_folder', { folderId }),
  loadOlder: (folderId: number) => invoke<number>('load_older', { folderId }),
  listThreads: (folderIds: number[], limit = 100, offset = 0) =>
    invoke<Thread[]>('list_threads', { folderIds, limit, offset }),

  getMessage: (id: number, forceRemote = false) => invoke<MessageBody>('get_message', { id, forceRemote }),
  getThread: (accountId: string, threadId: string, forceRemote = false) =>
    invoke<MessageBody[]>('get_thread', { accountId, threadId, forceRemote }),
  threadRows: (accountId: string, threadId: string, folderIds: number[]) =>
    invoke<number[]>('thread_rows', { accountId, threadId, folderIds }),
  setThreadPinned: (accountId: string, threadId: string, pinned: boolean) =>
    invoke<void>('set_thread_pinned', { accountId, threadId, pinned }),

  setSeen: (ids: number[], seen: boolean) => invoke<void>('set_seen', { ids, seen }),
  setFlagged: (ids: number[], flagged: boolean) => invoke<void>('set_flagged', { ids, flagged }),
  moveMessages: (ids: number[], target: MoveTarget) => invoke<void>('move_messages', { ids, target }),

  sendMessage: (message: OutgoingMessage) => invoke<string>('send_message', { message }),
  saveDraft: (message: OutgoingMessage, replaces: string | null) => invoke<string>('save_draft', { message, replaces }),
  deleteDraft: (accountId: string, messageId: string) => invoke<void>('delete_draft', { accountId, messageId }),
  pickAttachments: () => invoke<PickedFile[]>('pick_attachments'),
  inspectFiles: (paths: string[]) => invoke<PickedFile[]>('inspect_files', { paths }),
  saveAttachment: (messageId: number, index: number) => invoke<boolean>('save_attachment', { messageId, index }),
  readAttachment: (messageId: number, index: number) => invoke<ArrayBuffer>('read_attachment', { messageId, index }),
  openAttachment: (messageId: number, index: number) => invoke<void>('open_attachment', { messageId, index }),
  pickCustomSound: () => invoke<string | null>('pick_custom_sound'),
  readCustomSound: () => invoke<ArrayBuffer>('read_custom_sound'),
  openAttachmentWith: (messageId: number, index: number) => invoke<boolean>('open_attachment_with', { messageId, index }),

  search: (query: string, accountId: string | null = null) => invoke<MessageSummary[]>('search', { query, accountId }),
  contacts: (prefix: string) => invoke<EmailAddress[]>('contacts', { prefix }),

  listSignatures: () => invoke<Signature[]>('list_signatures'),
  saveSignature: (signature: Signature) => invoke<Signature>('save_signature', { signature }),
  deleteSignature: (id: string) => invoke<void>('delete_signature', { id }),

  getSettings: () => invoke<SettingsView>('get_settings'),
  saveSettings: (settings: Settings, autostart: boolean) => invoke<void>('save_settings', { settings, autostart }),
  trustSender: (email: string) => invoke<void>('trust_sender', { email }),
  openExternal: (url: string) => invoke<void>('open_external', { url }),
  unreadCount: () => invoke<number>('unread_count'),
  /** Path of the Cornamusa executable, for the MCP client configuration. */
  mcpCommand: () => invoke<string>('mcp_command'),
};

export function onMailEvent(cb: (e: MailEvent) => void): Promise<UnlistenFn> {
  return listen<MailEvent>('mail://event', (e) => cb(e.payload));
}

export function onAppEvent<T>(name: 'app://compose' | 'app://settings' | 'app://compose-draft', cb: (payload: T) => void): Promise<UnlistenFn> {
  return listen<T>(name, (e) => cb(e.payload));
}
