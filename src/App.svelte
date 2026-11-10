<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import { app } from './lib/state.svelte';
  import { onAppEvent, onMailEvent, type Settings as SettingsT } from './lib/api';
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
    <Composer init={app.composer} />
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
