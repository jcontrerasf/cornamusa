<script lang="ts">
  import { app, UNIFIED_LABELS } from '../state.svelte';
  import type { Folder, FolderRole } from '../api';
  import {
    Inbox, Send, FileText, Archive, ShieldAlert, Trash2, Folder as FolderIcon, Star, Layers,
    PenSquare, Settings, Plus, ChevronDown, ChevronRight, RefreshCw, AlertCircle, PanelLeftClose, PanelLeftOpen,
  } from '@lucide/svelte';

  const ICONS: Record<FolderRole, typeof Inbox> = {
    inbox: Inbox, sent: Send, drafts: FileText, archive: Archive, junk: ShieldAlert,
    trash: Trash2, all: Layers, flagged: Star, other: FolderIcon,
  };
  const UNIFIED: FolderRole[] = ['inbox', 'sent', 'drafts', 'archive', 'junk', 'trash'];

  let collapsed = $state<Record<string, boolean>>({});

  function isActiveFolder(f: Folder) {
    return app.view.kind === 'folder' && app.view.folderId === f.id;
  }
  function isActiveUnified(role: FolderRole) {
    return app.view.kind === 'unified' && app.view.role === role;
  }
  function showUnread(role: FolderRole) {
    return role === 'inbox' || role === 'junk' || role === 'other';
  }
  // Collapsed mode: names move to the tooltip.
  function tip(name: string, unread: number) {
    return mini && unread > 0 ? `${name} (${unread})` : name;
  }

  const mini = $derived(app.sidebarCollapsed);
</script>

<aside class="sidebar" class:mini>
  <div class="brand">
    <img src="/icon.svg" alt="" width="26" height="26" />
    {#if !mini}
      <span>Cornamusa</span>
      <button class="icon-btn toggle" title="Reducir a iconos (B)" onclick={() => app.toggleSidebar()}><PanelLeftClose size={17} /></button>
    {/if}
  </div>

  <button class="btn primary compose" onclick={() => app.compose()} title="Redactar (C)">
    <PenSquare size={16} />{#if !mini} Redactar{/if}
  </button>

  <nav class="scroll">
    {#if app.accounts.length > 1}
      <div class="section-label">Todas las cuentas</div>
      {#each UNIFIED as role}
        {@const Icon = ICONS[role]}
        {@const unread = showUnread(role) ? app.unifiedUnread(role) : 0}
        <button
          class="item"
          class:active={isActiveUnified(role)}
          title={mini ? tip(UNIFIED_LABELS[role] ?? role, unread) : undefined}
          onclick={() => app.setView({ kind: 'unified', role })}
        >
          <Icon size={17} />
          <span class="name">{UNIFIED_LABELS[role]}</span>
          {#if unread > 0}<span class="count">{unread}</span>{/if}
        </button>
      {/each}
    {/if}

    {#each app.accounts as account (account.id)}
      <div class="account">
        <button
          class="account-head"
          title={mini ? account.displayName || account.email : undefined}
          onclick={() => (collapsed[account.id] = !collapsed[account.id])}
        >
          {#if collapsed[account.id]}<ChevronRight size={14} />{:else}<ChevronDown size={14} />{/if}
          <span class="dot" style:background={account.color}></span>
          <span class="account-name" title={account.email}>{account.displayName || account.email}</span>
          {#if app.syncErrors[account.id]}
            <span class="warn" title={app.syncErrors[account.id]}><AlertCircle size={14} /></span>
          {:else if app.syncing[account.id]}
            <span class="faint"><RefreshCw size={13} class="spin" /></span>
          {/if}
        </button>
        {#if !collapsed[account.id]}
          {#each app.folders[account.id] ?? [] as f (f.id)}
            {@const Icon = ICONS[f.role]}
            <button
              title={mini ? tip(f.name, showUnread(f.role) ? f.unread : 0) : undefined}
              class="item"
              class:active={isActiveFolder(f) || (app.accounts.length === 1 && f.role !== 'other' && isActiveUnified(f.role))}
              onclick={() => app.setView({ kind: 'folder', accountId: account.id, folderId: f.id })}
            >
              <Icon size={17} />
              <span class="name" title={f.name}>{f.name}</span>
              {#if showUnread(f.role) && f.unread > 0}<span class="count">{f.unread}</span>{/if}
              {#if f.role === 'drafts' && f.total > 0}<span class="count soft">{f.total}</span>{/if}
            </button>
          {/each}
          {#if (app.folders[account.id] ?? []).length === 0}
            <div class="empty faint">Sincronizando…</div>
          {/if}
        {/if}
      </div>
    {/each}
  </nav>

  <div class="footer">
    {#if mini}
      <button class="icon-btn" title="Agregar cuenta" onclick={() => (app.showWizard = true)}><Plus size={18} /></button>
    {:else}
      <button class="btn ghost" onclick={() => (app.showWizard = true)}><Plus size={16} /> Agregar cuenta</button>
    {/if}
    <button class="icon-btn" title="Configuración" onclick={() => (app.showSettings = 'general')}><Settings size={18} /></button>
    {#if mini}
      <button class="icon-btn" title="Expandir barra lateral (B)" onclick={() => app.toggleSidebar()}><PanelLeftOpen size={18} /></button>
    {/if}
  </div>
</aside>

<style>
  .sidebar {
    width: 248px; flex-shrink: 0; display: flex; flex-direction: column;
    background: var(--surface-2); border-right: 1px solid var(--border); min-height: 0;
    transition: width 0.15s ease;
  }
  .toggle { margin: -4px -8px -4px auto; color: var(--text-3); }

  /* Icons-only mode */
  .mini { width: 60px; }
  .mini .brand { justify-content: center; padding: 16px 0 10px; }
  .mini .compose { margin: 6px 10px 12px; padding: 10px 0; }
  .mini .scroll { padding: 0 6px 8px; overflow-x: hidden; }
  .mini .section-label { height: 1px; padding: 0; margin: 8px 6px; background: var(--border); font-size: 0; }
  .mini .item { justify-content: center; padding: 8px 0; position: relative; }
  .mini .name { display: none; }
  .mini .count {
    position: absolute; top: 2px; right: 4px; min-width: 16px; height: 16px; padding: 0 4px; border-radius: 8px;
    background: var(--accent); color: var(--accent-text); font-size: 10px; line-height: 16px; text-align: center;
  }
  .mini .count.soft { background: var(--surface-3); color: var(--text-2); }
  .mini .account { margin-top: 6px; border-top: 1px solid var(--border); padding-top: 4px; }
  .mini .account-head { justify-content: center; gap: 4px; padding: 6px 0; }
  .mini .account-head > :global(svg:first-child),
  .mini .account-name { display: none; }
  .mini .empty { display: none; }
  .mini .footer { flex-direction: column; gap: 4px; padding: 8px 0; }
  .brand { display: flex; align-items: center; gap: 9px; padding: 16px 18px 10px; font-weight: 700; font-size: 16px; letter-spacing: -0.01em; }
  .compose { margin: 6px 14px 12px; justify-content: center; padding: 10px; border-radius: 12px; font-size: 14px; box-shadow: var(--shadow); }
  .scroll { flex: 1; overflow-y: auto; padding: 0 8px 8px; }
  .section-label { font-size: 11px; text-transform: uppercase; letter-spacing: 0.06em; color: var(--text-3); padding: 10px 10px 4px; font-weight: 600; }
  .item {
    width: 100%; display: flex; align-items: center; gap: 10px; padding: 7px 10px; border-radius: 8px;
    border: none; background: transparent; color: var(--text-2); text-align: left;
  }
  .item:hover { background: var(--surface-3); color: var(--text); }
  .item.active { background: var(--accent-soft); color: var(--accent); font-weight: 600; }
  .name { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .count { font-size: 12px; font-weight: 600; color: var(--accent); }
  .count.soft { color: var(--text-3); font-weight: 500; }
  .account { margin-top: 8px; }
  .account-head {
    width: 100%; display: flex; align-items: center; gap: 7px; padding: 6px 8px; border: none; background: none;
    color: var(--text-3); font-size: 12px; font-weight: 600; text-transform: uppercase; letter-spacing: 0.04em;
  }
  .account-name { flex: 1; text-align: left; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .dot { width: 8px; height: 8px; border-radius: 50%; flex-shrink: 0; }
  .warn { color: var(--danger); display: grid; }
  .empty { padding: 6px 12px; font-size: 12.5px; }
  .footer { display: flex; align-items: center; justify-content: space-between; padding: 10px 12px; border-top: 1px solid var(--border); }
</style>
