import { api, type Account, type Folder, type MessageBody, type MessageSummary, type SettingsView, type Signature, type Thread, type FolderRole, type EmailAddress, type MoveTarget } from './api';

export type View =
  | { kind: 'unified'; role: FolderRole }
  | { kind: 'folder'; accountId: string; folderId: number }
  | { kind: 'search'; query: string };

export type ComposeMode = 'new' | 'reply' | 'replyAll' | 'forward' | 'draft';

export interface ComposeInit {
  mode: ComposeMode;
  accountId?: string;
  to?: EmailAddress[];
  source?: MessageBody;
}

export interface Toast { id: number; kind: 'info' | 'error' | 'success'; text: string }

function errorText(e: unknown): string {
  return typeof e === 'string' ? e : e instanceof Error ? e.message : JSON.stringify(e);
}

class AppState {
  accounts = $state<Account[]>([]);
  folders = $state<Record<string, Folder[]>>({});
  view = $state<View>({ kind: 'unified', role: 'inbox' });
  threads = $state<Thread[]>([]);
  searchResults = $state<MessageSummary[]>([]);
  loadingList = $state(false);
  selected = $state<{ accountId: string; threadId: string } | null>(null);
  settings = $state<SettingsView | null>(null);
  signatures = $state<Signature[]>([]);
  syncing = $state<Record<string, boolean>>({});
  syncErrors = $state<Record<string, string | null>>({});
  composer = $state<ComposeInit | null>(null);
  showSettings = $state<null | 'general' | 'accounts' | 'signatures'>(null);
  showWizard = $state(false);
  toasts = $state<Toast[]>([]);
  pageSize = $state(100);

  private toastId = 0;

  toast(text: string, kind: Toast['kind'] = 'info') {
    const id = ++this.toastId;
    this.toasts.push({ id, kind, text });
    setTimeout(() => (this.toasts = this.toasts.filter((t) => t.id !== id)), kind === 'error' ? 7000 : 3500);
  }

  error(e: unknown) {
    this.toast(errorText(e), 'error');
  }

  /** OS preference, tracked live; only used when the theme setting is 'system'. */
  systemDark = $state(typeof matchMedia === 'function' && matchMedia('(prefers-color-scheme: dark)').matches);

  /** Whether the UI is currently rendered with the dark palette. */
  get isDark(): boolean {
    const theme = this.settings?.theme ?? 'system';
    return theme === 'system' ? this.systemDark : theme === 'dark';
  }

  get sidebarCollapsed(): boolean {
    return this.settings?.sidebarCollapsed ?? false;
  }

  async toggleSidebar() {
    if (!this.settings) return;
    this.settings.sidebarCollapsed = !this.settings.sidebarCollapsed;
    await this.saveSettings();
  }

  get chatView(): boolean {
    return this.settings?.chatView ?? false;
  }

  async toggleChatView() {
    if (!this.settings) return;
    this.settings.chatView = !this.settings.chatView;
    await this.saveSettings();
  }

  async saveSettings() {
    if (!this.settings) return;
    const { autostart, ...settings } = this.settings;
    try {
      await api.saveSettings(settings, autostart);
    } catch (e) {
      this.error(e);
    }
  }

  async init() {
    const [accounts, settings, signatures] = await Promise.all([api.listAccounts(), api.getSettings(), api.listSignatures()]);
    this.accounts = accounts;
    this.settings = settings;
    this.signatures = signatures;
    await this.reloadFolders();
    await this.reloadList();
    if (accounts.length === 0) this.showWizard = true;
  }

  async reloadAccounts() {
    this.accounts = await api.listAccounts();
    await this.reloadFolders();
    if (this.view.kind === 'folder' && !this.accounts.some((a) => a.id === (this.view as { accountId: string }).accountId)) {
      this.view = { kind: 'unified', role: 'inbox' };
    }
    await this.reloadList();
  }

  async reloadFolders(accountId?: string) {
    const ids = accountId ? [accountId] : this.accounts.map((a) => a.id);
    const results = await Promise.all(ids.map((id) => api.listFolders(id).catch(() => [] as Folder[])));
    const next = { ...this.folders };
    ids.forEach((id, i) => (next[id] = results[i]));
    for (const key of Object.keys(next)) if (!this.accounts.some((a) => a.id === key)) delete next[key];
    this.folders = next;
  }

  account(id: string): Account | undefined {
    return this.accounts.find((a) => a.id === id);
  }

  folder(id: number): Folder | undefined {
    for (const list of Object.values(this.folders)) {
      const f = list.find((f) => f.id === id);
      if (f) return f;
    }
  }

  /** Folder ids behind the current view (several for the unified views). */
  get viewFolderIds(): number[] {
    const v = this.view;
    if (v.kind === 'folder') return [v.folderId];
    if (v.kind === 'unified') {
      return Object.values(this.folders).flatMap((list) => list.filter((f) => f.role === v.role).map((f) => f.id));
    }
    return [];
  }

  get viewTitle(): string {
    const v = this.view;
    if (v.kind === 'search') return `Resultados: «${v.query}»`;
    if (v.kind === 'folder') return this.folder(v.folderId)?.name ?? 'Carpeta';
    return UNIFIED_LABELS[v.role] ?? 'Correo';
  }

  get viewRole(): FolderRole | null {
    const v = this.view;
    if (v.kind === 'unified') return v.role;
    if (v.kind === 'folder') return this.folder(v.folderId)?.role ?? null;
    return null;
  }

  isOwnAddress(email: string): boolean {
    return this.accounts.some((a) => a.email.toLowerCase() === email.toLowerCase());
  }

  unifiedUnread(role: FolderRole): number {
    return Object.values(this.folders).flatMap((l) => l.filter((f) => f.role === role)).reduce((n, f) => n + f.unread, 0);
  }

  async setView(v: View) {
    this.view = v;
    this.selected = null;
    this.pageSize = 100;
    await this.reloadList();
    if (v.kind === 'folder') {
      const f = this.folder(v.folderId);
      // Special folders are kept in sync by the background worker; others sync on open.
      if (f && (f.role === 'other' || f.role === 'all' || f.role === 'flagged')) {
        api.syncFolder(f.id).catch((e) => this.error(e));
      }
    }
  }

  async reloadList() {
    const v = this.view;
    this.loadingList = true;
    try {
      if (v.kind === 'search') {
        this.searchResults = await api.search(v.query);
        this.threads = [];
      } else {
        this.threads = await api.listThreads(this.viewFolderIds, this.pageSize, 0);
      }
    } catch (e) {
      this.error(e);
    } finally {
      this.loadingList = false;
    }
  }

  async loadMore() {
    this.pageSize += 100;
    await this.reloadList();
    // Reached the end of what's cached: ask the server for older mail.
    if (this.threads.length < this.pageSize && this.view.kind !== 'search') {
      let fetched = 0;
      for (const id of this.viewFolderIds) fetched += await api.loadOlder(id).catch(() => 0);
      if (fetched > 0) await this.reloadList();
    }
  }

  async search(query: string) {
    if (!query.trim()) return this.setView({ kind: 'unified', role: 'inbox' });
    await this.setView({ kind: 'search', query: query.trim() });
  }

  async refresh() {
    await Promise.all(this.accounts.map((a) => api.syncAccount(a.id).catch((e) => this.error(e))));
  }

  select(accountId: string, threadId: string) {
    this.selected = { accountId, threadId };
  }

  /** Rows of a thread that belong to the current view (so archiving from the inbox doesn't
   * touch the user's own replies in Sent). */
  async rowsOf(t: { accountId: string; threadId: string }): Promise<number[]> {
    const folderIds = this.view.kind === 'search' ? [] : this.viewFolderIds;
    return api.threadRows(t.accountId, t.threadId, folderIds);
  }

  async moveThread(t: { accountId: string; threadId: string }, target: MoveTarget) {
    const labels: Record<MoveTarget, string> = {
      archive: 'Archivado', trash: 'Movido a la papelera', junk: 'Marcado como no deseado', inbox: 'Movido a Recibidos',
    };
    try {
      const ids = await this.rowsOf(t);
      // Optimistic: drop it from the list right away.
      this.threads = this.threads.filter((x) => !(x.threadId === t.threadId && x.accountId === t.accountId));
      if (this.selected?.threadId === t.threadId) this.selected = null;
      await api.moveMessages(ids, target);
      this.toast(labels[target], 'success');
    } catch (e) {
      this.error(e);
      await this.reloadList();
    }
  }

  async setThreadSeen(t: { accountId: string; threadId: string }, seen: boolean) {
    try {
      const ids = await api.threadRows(t.accountId, t.threadId, []);
      await api.setSeen(ids, seen);
    } catch (e) {
      this.error(e);
    }
  }

  async setThreadFlagged(t: Thread, flagged: boolean) {
    try {
      await api.setFlagged([t.latestId], flagged);
    } catch (e) {
      this.error(e);
    }
  }

  compose(init: ComposeInit = { mode: 'new' }) {
    this.composer = init;
  }

  async reloadSignatures() {
    this.signatures = await api.listSignatures();
  }

  defaultSignature(accountId: string): Signature | undefined {
    return (
      this.signatures.find((s) => s.accountId === accountId && s.isDefault) ??
      this.signatures.find((s) => s.accountId === null && s.isDefault)
    );
  }
}

export const UNIFIED_LABELS: Partial<Record<FolderRole, string>> = {
  inbox: 'Recibidos',
  sent: 'Enviados',
  drafts: 'Borradores',
  archive: 'Archivo',
  junk: 'No deseado',
  trash: 'Papelera',
};

export const app = new AppState();

if (typeof matchMedia === 'function') {
  matchMedia('(prefers-color-scheme: dark)').addEventListener('change', (e) => (app.systemDark = e.matches));
}
