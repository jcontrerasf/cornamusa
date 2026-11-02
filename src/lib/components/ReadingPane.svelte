<script lang="ts">
  import { app } from '../state.svelte';
  import { api, type MessageBody } from '../api';
  import ClassicView from './ClassicView.svelte';
  import ChatView from './ChatView.svelte';
  import {
    Reply, ReplyAll, Forward, Archive, Trash2, ShieldAlert, Inbox, Mail, Star, MessagesSquare, Rows3, ImageOff,
  } from '@lucide/svelte';

  let messages = $state<MessageBody[]>([]);
  let loading = $state(false);
  let forceRemote = $state(false);
  let loadedKey = '';

  const sel = $derived(app.selected);
  const account = $derived(sel ? app.account(sel.accountId) : undefined);
  const last = $derived(messages[messages.length - 1]);
  const flagged = $derived(messages.some((m) => m.summary.flagged));
  const role = $derived(app.viewRole);
  const anyBlocked = $derived(messages.some((m) => m.remoteContentBlocked));

  async function load() {
    if (!sel) return;
    const key = `${sel.accountId}|${sel.threadId}`;
    loading = true;
    try {
      const result = await api.getThread(sel.accountId, sel.threadId, forceRemote);
      if (`${app.selected?.accountId}|${app.selected?.threadId}` !== key) return; // selection changed meanwhile
      messages = result;
      loadedKey = key;
      const unread = result.filter((m) => !m.summary.seen).map((m) => m.summary.id);
      if (unread.length) {
        result.forEach((m) => (m.summary.seen = true));
        api.setSeen(unread, true).catch((e) => app.error(e));
      }
    } catch (e) {
      app.error(e);
    } finally {
      loading = false;
    }
  }

  $effect(() => {
    const key = sel ? `${sel.accountId}|${sel.threadId}` : '';
    if (key !== loadedKey) {
      forceRemote = false;
      if (!key) {
        messages = [];
        loadedKey = '';
      } else {
        messages = [];
        load();
      }
    }
  });

  // Reload when the mailbox of this account changes (new reply arrived, flags...).
  export function refreshIfShowing(accountId: string) {
    if (sel && sel.accountId === accountId && !loading) load();
  }

  function showImages() {
    forceRemote = true;
    load();
  }

  async function trustSender() {
    const from = last?.summary.from?.email;
    if (!from) return;
    await api.trustSender(from).catch((e) => app.error(e));
    showImages();
    app.toast(`Las imágenes de ${from} se mostrarán siempre`, 'success');
  }

  export function reply(all = false) {
    if (last) app.compose({ mode: all ? 'replyAll' : 'reply', accountId: last.summary.accountId, source: last });
  }
  export function forward() {
    if (last) app.compose({ mode: 'forward', accountId: last.summary.accountId, source: last });
  }
  export function move(target: 'archive' | 'trash' | 'junk' | 'inbox') {
    if (sel) app.moveThread(sel, target);
  }
  export async function toggleFlag() {
    if (!last) return;
    const next = !flagged;
    messages.forEach((m) => (m.summary.flagged = false));
    last.summary.flagged = next;
    await api.setFlagged([last.summary.id], next).catch((e) => app.error(e));
  }
  export async function markUnread() {
    if (!sel) return;
    await app.setThreadSeen(sel, false);
    app.selected = null;
  }
</script>

<section class="pane">
  {#if !sel}
    <div class="placeholder">
      <img src="/icon.svg" alt="" width="72" height="72" />
      <p>Selecciona una conversación</p>
      <p class="faint hint"><kbd>C</kbd> redactar · <kbd>/</kbd> buscar · <kbd>J</kbd>/<kbd>K</kbd> navegar · <kbd>V</kbd> vista chat · <kbd>B</kbd> barra lateral</p>
    </div>
  {:else}
    <header class="toolbar">
      <div class="group">
        <button class="icon-btn" title="Responder (R)" onclick={() => reply(false)} disabled={!last}><Reply size={18} /></button>
        <button class="icon-btn" title="Responder a todos (A)" onclick={() => reply(true)} disabled={!last}><ReplyAll size={18} /></button>
        <button class="icon-btn" title="Reenviar (F)" onclick={forward} disabled={!last}><Forward size={18} /></button>
      </div>
      <div class="group">
        {#if role === 'inbox'}
          <button class="icon-btn" title="Archivar (E)" onclick={() => move('archive')}><Archive size={18} /></button>
        {:else if role !== 'sent' && role !== 'drafts'}
          <button class="icon-btn" title="Mover a Recibidos" onclick={() => move('inbox')}><Inbox size={18} /></button>
        {/if}
        {#if role !== 'junk'}
          <button class="icon-btn" title="No deseado" onclick={() => move('junk')}><ShieldAlert size={18} /></button>
        {/if}
        <button class="icon-btn" title={role === 'trash' ? 'Eliminar definitivamente' : 'Eliminar (#)'} onclick={() => move('trash')}><Trash2 size={18} /></button>
      </div>
      <div class="group">
        <button class="icon-btn" title="Marcar como no leído (U)" onclick={markUnread}><Mail size={18} /></button>
        <button class="icon-btn" class:active={flagged} title="Destacar (S)" onclick={toggleFlag}>
          <Star size={18} fill={flagged ? 'currentColor' : 'none'} />
        </button>
      </div>
      {#if role === 'drafts' && last}
        <button class="btn primary" onclick={() => app.compose({ mode: 'draft', accountId: last.summary.accountId, source: last })}>Editar borrador</button>
      {/if}
      <div class="spacer"></div>
      <div class="view-toggle" role="group" aria-label="Modo de vista">
        <button class:on={!app.chatView} onclick={() => app.chatView && app.toggleChatView()} title="Vista clásica">
          <Rows3 size={15} /> Clásica
        </button>
        <button class:on={app.chatView} onclick={() => !app.chatView && app.toggleChatView()} title="Vista de mensajería">
          <MessagesSquare size={15} /> Chat
        </button>
      </div>
    </header>

    {#if anyBlocked && !forceRemote}
      <div class="privacy">
        <ImageOff size={16} />
        <span>Se bloquearon imágenes remotas para evitar que el remitente sepa cuándo abres el correo.</span>
        <button class="btn ghost" onclick={showImages}>Mostrar imágenes</button>
        {#if last?.summary.from}<button class="btn ghost" onclick={trustSender}>Siempre de este remitente</button>{/if}
      </div>
    {/if}

    {#if loading && messages.length === 0}
      <div class="placeholder"><p class="faint">Cargando conversación…</p></div>
    {:else if account && messages.length}
      {#if app.chatView}
        <ChatView {messages} {account} allowRemote={forceRemote} onSent={() => load()} />
      {:else}
        <ClassicView {messages} allowRemote={forceRemote} />
      {/if}
    {/if}
  {/if}
</section>

<style>
  .pane { flex: 1; min-width: 0; display: flex; flex-direction: column; background: var(--bg); min-height: 0; }
  .placeholder { flex: 1; display: grid; place-content: center; justify-items: center; gap: 4px; color: var(--text-2); opacity: 0.9; }
  .placeholder img { opacity: 0.35; filter: grayscale(0.4); margin-bottom: 8px; }
  .hint { font-size: 12.5px; }
  .toolbar { display: flex; align-items: center; gap: 10px; padding: 8px 14px; border-bottom: 1px solid var(--border); background: var(--surface); }
  .group { display: flex; gap: 2px; padding-right: 10px; border-right: 1px solid var(--border); }
  .group:last-of-type { border-right: none; }
  .spacer { flex: 1; }
  .view-toggle { display: flex; background: var(--surface-2); border-radius: 9px; padding: 3px; gap: 2px; }
  .view-toggle button {
    display: flex; align-items: center; gap: 6px; border: none; background: transparent; color: var(--text-2);
    padding: 5px 11px; border-radius: 7px; font-size: 12.5px; font-weight: 600;
  }
  .view-toggle button.on { background: var(--surface); color: var(--accent); box-shadow: var(--shadow); }
  .privacy {
    display: flex; align-items: center; gap: 10px; padding: 8px 16px; font-size: 12.5px;
    background: color-mix(in srgb, var(--warning) 12%, var(--surface)); color: var(--text-2); border-bottom: 1px solid var(--border);
  }
  .privacy span { flex: 1; }
  .privacy .btn { padding: 4px 8px; font-size: 12.5px; color: var(--accent); }
</style>
