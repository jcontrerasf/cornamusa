<script lang="ts">
  import { app, UNIFIED_LABELS } from '../state.svelte';
  import type { Folder, FolderRole } from '../api';
  import { FOLDER_ICONS as ICONS, showsUnread as showUnread } from '../folders';
  import {
    PenSquare, Settings, Plus, ChevronDown, ChevronRight, RefreshCw, AlertCircle, PanelLeftClose, PanelLeftOpen,
    Eye, EyeOff,
  } from '@lucide/svelte';

  const UNIFIED: FolderRole[] = ['inbox', 'sent', 'drafts', 'archive', 'junk', 'trash'];

  let collapsed = $state<Record<string, boolean>>({});
  /** Accounts whose hidden folders are expanded ("Más carpetas"), expanded sidebar only. */
  let showHidden = $state<Record<string, boolean>>({});

  function isActiveFolder(f: Folder) {
    return app.view.kind === 'folder' && app.view.folderId === f.id;
  }
  function isActive(f: Folder) {
    return isActiveFolder(f) || (app.accounts.length === 1 && f.role !== 'other' && isActiveUnified(f.role));
  }
  function isActiveUnified(role: FolderRole) {
    return app.view.kind === 'unified' && app.view.role === role;
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
          {@const all = app.folders[account.id] ?? []}
          {@const visible = all.filter((f) => !app.isHidden(f) || isActive(f))}
          {@const hidden = all.filter((f) => app.isHidden(f) && !isActive(f))}
          {@const hiddenUnread = hidden.reduce((n, f) => n + (showUnread(f.role) ? f.unread : 0), 0)}
          {#each visible as f (f.id)}{@render folderRow(f, false)}{/each}
          {#if !mini && hidden.length > 0}
            <button class="item more" onclick={() => (showHidden[account.id] = !showHidden[account.id])}>
              {#if showHidden[account.id]}<ChevronDown size={17} />{:else}<ChevronRight size={17} />{/if}
              <span class="name">{showHidden[account.id] ? 'Menos carpetas' : `Más carpetas (${hidden.length})`}</span>
              {#if !showHidden[account.id] && hiddenUnread > 0}<span class="count">{hiddenUnread}</span>{/if}
            </button>
            {#if showHidden[account.id]}
              {#each hidden as f (f.id)}{@render folderRow(f, true)}{/each}
            {/if}
          {/if}
          {#if (app.folders[account.id] ?? []).length === 0}
            <div class="empty faint">Sincronizando…</div>
          {/if}
        {/if}
      </div>
    {/each}
  </nav>

  {#snippet folderRow(f: Folder, isHiddenRow: boolean)}
    {@const Icon = ICONS[f.role]}
    <div class="row" class:hidden-folder={isHiddenRow}>
      <button
        title={mini ? tip(f.name, showUnread(f.role) ? f.unread : 0) : undefined}
        class="item"
        class:active={isActive(f)}
        onclick={() => app.setView({ kind: 'folder', accountId: f.accountId, folderId: f.id })}
      >
        <Icon size={17} />
        <span class="name" title={f.name}>{f.name}</span>
        {#if showUnread(f.role) && f.unread > 0}<span class="count">{f.unread}</span>{/if}
        {#if f.role === 'drafts' && f.total > 0}<span class="count soft">{f.total}</span>{/if}
      </button>
      {#if !mini && f.role !== 'inbox'}
        <button
          class="icon-btn eye"
          title={app.isHidden(f) ? 'Mostrar en la barra lateral' : 'Ocultar de la barra lateral'}
          onclick={() => app.toggleHidden(f)}
        >
          {#if app.isHidden(f)}<Eye size={14} />{:else}<EyeOff size={14} />{/if}
        </button>
      {/if}
    </div>
  {/snippet}

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
  .row { position: relative; }
  .row .eye {
    position: absolute; right: 4px; top: 50%; transform: translateY(-50%); width: 26px; height: 26px;
    color: var(--text-3); background: var(--surface-3); opacity: 0; pointer-events: none;
  }
  .row:hover .eye { opacity: 1; pointer-events: auto; }
  .row .eye:hover { color: var(--text); }
  .hidden-folder .item:not(.active) { opacity: 0.6; }
  .more { color: var(--text-3); font-size: 13px; }
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
