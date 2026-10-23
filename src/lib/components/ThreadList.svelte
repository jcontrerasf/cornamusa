<script lang="ts">
  import { app } from '../state.svelte';
  import type { Thread, MessageSummary } from '../api';
  import { displayName, firstName, formatListDate } from '../util';
  import Avatar from './Avatar.svelte';
  import { Search, RefreshCw, Paperclip, Star, Archive, Trash2, MailOpen, Mail, MessagesSquare, X } from '@lucide/svelte';

  let query = $state('');
  let searchEl: HTMLInputElement | undefined = $state();

  export function focusSearch() {
    searchEl?.focus();
  }

  const refreshing = $derived(Object.values(app.syncing).some(Boolean));

  const outgoing = $derived(app.viewRole === 'sent' || app.viewRole === 'drafts');

  function names(list: { name: string | null; email: string }[]): string {
    if (list.length === 0) return '(sin destinatarios)';
    if (list.length === 1) return displayName(list[0]);
    return list.slice(0, 3).map(firstName).join(', ') + (list.length > 3 ? '…' : '');
  }

  function senders(t: Thread): string {
    if (outgoing) return 'Para: ' + names(t.recipients);
    const others = t.participants.filter((p) => !app.isOwnAddress(p.email));
    const list = others.length ? others : t.participants;
    if (list.length === 0) return '(sin remitente)';
    if (list.length === 1) return displayName(list[0]);
    return list.slice(0, 3).map(firstName).join(', ') + (list.length > 3 ? '…' : '');
  }

  function onSearch(e: Event) {
    e.preventDefault();
    app.search(query);
  }

  function clearSearch() {
    query = '';
    app.setView({ kind: 'unified', role: 'inbox' });
  }

  function openResult(m: MessageSummary) {
    app.select(m.accountId, m.threadId);
  }

  const isSelected = (t: { accountId: string; threadId: string }) =>
    app.selected?.threadId === t.threadId && app.selected?.accountId === t.accountId;

  function onScroll(e: Event) {
    const el = e.currentTarget as HTMLElement;
    if (!app.loadingList && el.scrollTop + el.clientHeight > el.scrollHeight - 200 && app.threads.length >= app.pageSize) {
      app.loadMore();
    }
  }
</script>

<section class="list">
  <form class="search" onsubmit={onSearch}>
    <Search size={16} />
    <input bind:this={searchEl} bind:value={query} placeholder="Buscar correo…" class="selectable" />
    {#if app.view.kind === 'search'}
      <button type="button" class="icon-btn" onclick={clearSearch} title="Limpiar"><X size={15} /></button>
    {/if}
  </form>

  <header>
    <h2>{app.viewTitle}</h2>
    <div class="tools">
      <button class="icon-btn" class:active={app.chatView} onclick={() => app.toggleChatView()} title="Vista de mensajería (V)">
        <MessagesSquare size={17} />
      </button>
      <button class="icon-btn" onclick={() => app.refresh()} title="Actualizar" disabled={refreshing}>
        <RefreshCw size={16} class={refreshing ? 'spin' : ''} />
      </button>
    </div>
  </header>

  <div class="rows" onscroll={onScroll}>
    {#if app.view.kind === 'search'}
      {#each app.searchResults as m (m.id)}
        <button class="row" class:unread={!m.seen} class:selected={isSelected(m)} onclick={() => openResult(m)}>
          <Avatar address={m.from} />
          <div class="body">
            <div class="line1">
              <span class="from">{displayName(m.from)}</span>
              <span class="date">{formatListDate(m.date)}</span>
            </div>
            <div class="subject">{m.subject || '(sin asunto)'}</div>
            <div class="preview">{m.preview}</div>
          </div>
        </button>
      {:else}
        <div class="empty">{app.loadingList ? 'Buscando…' : 'Sin resultados'}</div>
      {/each}
    {:else}
      {#each app.threads as t (t.accountId + t.threadId)}
        {@const account = app.account(t.accountId)}
        <div class="row-wrap">
          <button class="row" class:unread={t.unreadCount > 0} class:selected={isSelected(t)} onclick={() => app.select(t.accountId, t.threadId)}>
            {#if app.accounts.length > 1 && account}<span class="acct" style:background={account.color}></span>{/if}
            <Avatar address={outgoing ? (t.recipients[0] ?? t.participants[0]) : (t.participants.find((p) => !app.isOwnAddress(p.email)) ?? t.participants[0])} />
            <div class="body">
              <div class="line1">
                <span class="from">{senders(t)}</span>
                {#if t.messageCount > 1}<span class="n">{t.messageCount}</span>{/if}
                <span class="date">{formatListDate(t.lastDate)}</span>
              </div>
              <div class="subject">
                {#if t.unreadCount > 0}<span class="dot"></span>{/if}
                {t.subject || '(sin asunto)'}
              </div>
              <div class="preview">{t.preview}</div>
            </div>
            <div class="marks">
              {#if t.flagged}<Star size={14} fill="currentColor" class="star" />{/if}
              {#if t.hasAttachments}<Paperclip size={14} />{/if}
            </div>
          </button>
          <div class="hover-actions">
            <button class="icon-btn" title="Archivar (E)" onclick={() => app.moveThread(t, 'archive')}><Archive size={15} /></button>
            <button class="icon-btn" title="Eliminar (#)" onclick={() => app.moveThread(t, 'trash')}><Trash2 size={15} /></button>
            <button class="icon-btn" title={t.unreadCount ? 'Marcar como leído' : 'Marcar como no leído'}
              onclick={() => app.setThreadSeen(t, t.unreadCount > 0)}>
              {#if t.unreadCount}<MailOpen size={15} />{:else}<Mail size={15} />{/if}
            </button>
            <button class="icon-btn" title="Destacar (S)" onclick={() => app.setThreadFlagged(t, !t.flagged)}>
              <Star size={15} fill={t.flagged ? 'currentColor' : 'none'} />
            </button>
          </div>
        </div>
      {:else}
        <div class="empty">
          {#if app.loadingList}Cargando…{:else if app.accounts.length === 0}Agrega una cuenta para empezar{:else}No hay mensajes aquí 🎉{/if}
        </div>
      {/each}
    {/if}
  </div>
</section>

<style>
  .list { width: 380px; flex-shrink: 0; display: flex; flex-direction: column; border-right: 1px solid var(--border); background: var(--surface); min-height: 0; }
  .search {
    margin: 12px 12px 4px; display: flex; align-items: center; gap: 8px; padding: 0 6px 0 12px;
    background: var(--surface-2); border-radius: 10px; color: var(--text-3); border: 1px solid transparent;
  }
  .search:focus-within { border-color: var(--accent); background: var(--surface); }
  .search input { flex: 1; border: none; background: none; outline: none; padding: 9px 0; }
  header { display: flex; align-items: center; justify-content: space-between; padding: 8px 12px 8px 18px; }
  h2 { margin: 0; font-size: 18px; font-weight: 700; letter-spacing: -0.01em; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .tools { display: flex; gap: 2px; }
  .rows { flex: 1; overflow-y: auto; }
  .row-wrap { position: relative; }
  .row {
    width: 100%; display: flex; gap: 11px; padding: 11px 14px 11px 16px; border: none; background: transparent;
    text-align: left; border-bottom: 1px solid var(--border); position: relative;
  }
  .row:hover { background: var(--surface-2); }
  .row.selected { background: var(--accent-soft); }
  .acct { position: absolute; left: 0; top: 10px; bottom: 10px; width: 3px; border-radius: 0 3px 3px 0; }
  .body { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px; }
  .line1 { display: flex; align-items: baseline; gap: 6px; }
  .from { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--text); font-weight: 500; }
  .unread .from, .unread .subject { font-weight: 700; }
  .n { font-size: 11px; color: var(--text-3); background: var(--surface-3); padding: 0 6px; border-radius: 8px; }
  .date { font-size: 12px; color: var(--text-3); flex-shrink: 0; }
  .unread .date { color: var(--accent); font-weight: 600; }
  .subject { color: var(--text); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; display: flex; align-items: center; gap: 6px; font-size: 13.5px; }
  .dot { width: 7px; height: 7px; border-radius: 50%; background: var(--accent); flex-shrink: 0; }
  .preview { color: var(--text-3); font-size: 12.5px; overflow: hidden; display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; line-height: 1.4; }
  .marks { display: flex; flex-direction: column; gap: 6px; color: var(--text-3); padding-top: 22px; }
  .marks :global(.star) { color: #f5a524; }
  .hover-actions {
    position: absolute; right: 10px; top: 8px; display: none; gap: 2px; background: var(--surface);
    border: 1px solid var(--border); border-radius: 9px; padding: 2px; box-shadow: var(--shadow);
  }
  .row-wrap:hover .hover-actions { display: flex; }
  .empty { padding: 48px 20px; text-align: center; color: var(--text-3); }
</style>
