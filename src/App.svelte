<script lang="ts">
  import { onDestroy, onMount, tick } from 'svelte';
  import { app } from './lib/state.svelte';
  import { api, onAppEvent, onMailEvent, type DraftChangeRequest, type DraftRequest, type DraftState, type Settings as SettingsT } from './lib/api';
  import type { ComposeInit } from './lib/state.svelte';
  import { paragraphsToHtml } from './lib/util';
  import { initialDraft } from './lib/compose';
  import Sidebar from './lib/components/Sidebar.svelte';
  import ThreadList from './lib/components/ThreadList.svelte';
  import ReadingPane from './lib/components/ReadingPane.svelte';
  import Composer from './lib/components/Composer.svelte';
  import Settings from './lib/components/Settings.svelte';
  import AccountWizard from './lib/components/AccountWizard.svelte';
  import Toasts from './lib/components/Toasts.svelte';
  import AttachmentViewer from './lib/components/AttachmentViewer.svelte';
  import { playSound, soundFor } from './lib/sounds';

  let list: ThreadList | undefined = $state();
  let pane: ReadingPane | undefined = $state();
  let composer: Composer | undefined = $state();
  let ready = $state(false);
  const unlisten: (() => void)[] = [];

  // Coalesce bursts of backend events into one list refresh.
  let refreshTimer: ReturnType<typeof setTimeout> | undefined;
  const pendingFolders = new Set<string>();
  function scheduleRefresh(accountId: string) {
    pendingFolders.add(accountId);
    clearTimeout(refreshTimer);
    refreshTimer = setTimeout(async () => {
      const ids = [...pendingFolders];
      pendingFolders.clear();
      for (const id of ids) await app.reloadFolders(id);
      await app.reloadList();
      ids.forEach((id) => pane?.refreshIfShowing(id));
    }, 250);
  }

  /** Draft prepared by Claude (MCP): a reply to a thread or a new message, opened for review. */
  async function openClaudeDraft(d: DraftRequest) {
    let init: ComposeInit;
    try {
      const base = {
        claudeId: d.id,
        to: d.to.length ? d.to : undefined,
        cc: d.cc.length ? d.cc : undefined,
        subject: d.subject ?? undefined,
        body: paragraphsToHtml(d.body),
        attachments: await api.inspectFiles(d.attachments),
      };
      if (d.replyTo) {
        const thread = await api.getThread(d.replyTo.accountId, d.replyTo.threadId);
        const source = thread[thread.length - 1];
        if (!source) throw new Error('No encontré el hilo al que responde el borrador');
        init = { ...base, mode: d.replyAll ? 'replyAll' : 'reply', accountId: d.replyTo.accountId, source };
      } else {
        init = { ...base, mode: 'new', accountId: d.accountId ?? undefined };
      }
    } catch (e) {
      return app.error(e);
    }
    if (app.composer) {
      // Never replace what the user is writing: open it when the current composer closes.
      app.pendingDrafts = [...app.pendingDrafts, init];
      app.toast('Claude preparó un borrador; se abrirá al cerrar el mensaje actual');
    } else {
      app.compose(init);
    }
  }

  /** Claude reads, changes or deletes a draft it prepared, wherever it is now: open in the
   *  composer, waiting in the queue, or closed and saved in Drafts. */
  async function changeClaudeDraft(c: DraftChangeRequest): Promise<DraftState> {
    await tick(); // a draft that was just opened has its composer mounted by now
    const isOpen = app.composer?.claudeId === c.id;
    const remove = c.action === 'delete';
    let draft: ComposeInit | undefined;
    let status: DraftState['status'] = 'open';
    if (isOpen) {
      if (remove) {
        await composer?.remove();
        return { status: 'deleted' };
      }
      draft = await composer?.current();
    } else {
      draft = app.pendingDrafts.find((p) => p.claudeId === c.id);
      status = draft ? 'queued' : 'saved';
      draft ??= app.claudeDrafts.get(c.id);
    }
    if (!draft) {
      throw new Error('Ese borrador ya no existe en Cornamusa (se envió, se descartó o la app se reinició). Si hace falta, crea uno nuevo con create_draft.');
    }
    if (c.action === 'read') {
      // Same values the composer shows (a reply never opened still derives them from its thread).
      const start = initialDraft(draft, app.account(draft.accountId ?? draft.source?.summary.accountId ?? ''), app.accounts[0]?.id ?? '');
      return {
        status, to: start.to, cc: start.cc, subject: start.subject, body: start.contentText,
        attachments: (draft.attachments ?? []).map((a) => a.path),
      };
    }
    if (remove && draft.draftId && draft.accountId) await api.deleteDraft(draft.accountId, draft.draftId);
    const next: ComposeInit | null = remove ? null : {
      ...draft,
      ...(c.to && { to: c.to }),
      ...(c.cc && { cc: c.cc }),
      ...(c.subject !== null && { subject: c.subject }),
      ...(c.body !== null && { body: paragraphsToHtml(c.body) }),
      ...(c.attachments && { attachments: await api.inspectFiles(c.attachments) }),
    };
    app.pendingDrafts = app.pendingDrafts.filter((p) => p.claudeId !== c.id);
    app.claudeDrafts.delete(c.id);
    if (!next) return { status: 'deleted' };
    if (isOpen || !app.composer) {
      app.compose(next);
      return { status: 'open' };
    }
    app.pendingDrafts = [...app.pendingDrafts, next];
    return { status: 'queued' };
  }

  // Requests from Claude run one at a time, in order: an edit sent right after a new draft
  // must find it already open.
  let claudeQueue: Promise<unknown> = Promise.resolve();
  function fromClaude(job: () => Promise<unknown>) {
    claudeQueue = claudeQueue.then(job).catch((e) => app.error(e));
  }

  async function answerDraftChange(c: DraftChangeRequest) {
    try {
      await api.mcpDraftResult(c.requestId, await changeClaudeDraft(c), null);
    } catch (e) {
      await api.mcpDraftResult(c.requestId, null, e instanceof Error ? e.message : String(e));
    }
  }

  $effect(() => {
    if (!app.composer && app.pendingDrafts.length) {
      const [next, ...rest] = app.pendingDrafts;
      app.pendingDrafts = rest;
      app.compose(next);
    }
  });

  $effect(() => {
    const theme = app.settings?.theme ?? 'system';
    if (theme === 'system') document.documentElement.removeAttribute('data-theme');
    else document.documentElement.dataset.theme = theme;
  });

  onMount(async () => {
    unlisten.push(
      await onMailEvent((e) => {
        switch (e.type) {
          case 'accountsChanged':
            app.reloadAccounts();
            break;
          case 'mailboxChanged':
            scheduleRefresh(e.accountId);
            break;
          case 'newMail': {
            // One sound per batch; the backend emits a single event for several new messages.
            const s = app.settings;
            if (s && s.notifications && !s.doNotDisturb && e.messages.length) playSound(soundFor(s, e.accountId));
            break;
          }
          case 'syncStatus':
            app.syncing[e.accountId] = e.syncing;
            if (!e.syncing) app.syncErrors[e.accountId] = e.error;
            break;
          case 'unreadCount':
            document.title = e.count > 0 ? `Cornamusa (${e.count})` : 'Cornamusa';
            break;
        }
      }),
      await onAppEvent('app://compose', () => app.compose()),
      await onAppEvent<DraftRequest>('app://compose-draft', (d) => fromClaude(() => openClaudeDraft(d))),
      await onAppEvent<DraftChangeRequest>('app://change-draft', (c) => fromClaude(() => answerDraftChange(c))),
      await onAppEvent<SettingsT>('app://settings', (s) => {
        if (app.settings) Object.assign(app.settings, s);
      }),
    );
    try {
      await app.init();
    } catch (e) {
      app.error(e);
    }
    ready = true;
  });

  onDestroy(() => unlisten.forEach((u) => u()));

  function moveSelection(delta: number) {
    const items = app.view.kind === 'search' ? app.searchResults : app.threads;
    if (!items.length) return;
    const idx = items.findIndex((t) => t.threadId === app.selected?.threadId && t.accountId === app.selected?.accountId);
    const next = items[Math.min(items.length - 1, Math.max(0, idx + delta))];
    app.select(next.accountId, next.threadId);
  }

  function onKey(e: KeyboardEvent) {
    const t = e.target as HTMLElement;
    if (t.closest('input, textarea, select, [contenteditable="true"]')) return;
    if (app.composer || app.showSettings || app.showWizard || app.viewer || e.ctrlKey || e.metaKey || e.altKey) return;
    const actions: Record<string, () => void> = {
      c: () => app.compose(),
      '/': () => list?.focusSearch(),
      j: () => moveSelection(1),
      k: () => moveSelection(-1),
      ArrowDown: () => moveSelection(1),
      ArrowUp: () => moveSelection(-1),
      r: () => pane?.reply(false),
      a: () => pane?.reply(true),
      f: () => pane?.forward(),
      e: () => pane?.move('archive'),
      '#': () => pane?.move('trash'),
      Delete: () => pane?.move('trash'),
      s: () => pane?.toggleFlag(),
      u: () => pane?.markUnread(),
      p: () => pane?.togglePin(),
      t: () => pane?.toggleTab(),
      v: () => app.toggleChatView(),
      b: () => app.toggleSidebar(),
      Escape: () => (app.selected = null),
    };
    const action = actions[e.key];
    if (action) {
      e.preventDefault();
      action();
    }
  }
</script>

<svelte:window onkeydown={onKey} />

<main class:ready>
  <Sidebar />
  <ThreadList bind:this={list} />
  <ReadingPane bind:this={pane} />
</main>

{#if app.composer}
  {#key app.composer}
    <Composer bind:this={composer} init={app.composer} />
  {/key}
{/if}
{#if app.showSettings}<Settings />{/if}
{#if app.showWizard}<AccountWizard />{/if}
{#if app.viewer}<AttachmentViewer />{/if}
<Toasts />

<style>
  main { display: flex; height: 100vh; opacity: 0; transition: opacity 0.2s; }
  main.ready { opacity: 1; }
</style>
