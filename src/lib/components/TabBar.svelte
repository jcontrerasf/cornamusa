<script lang="ts">
  // Conversations the user chose to keep open, as tabs under the reading pane. Clicking one
  // shows it whatever folder the list is on; tabs are saved with the settings.
  import { app, sameThread } from '../state.svelte';
  import type { OpenTab } from '../api';
  import { X } from '@lucide/svelte';

  const isCurrent = (t: OpenTab) => !!app.selected && sameThread(t, app.selected);

  function onAux(e: MouseEvent, t: OpenTab) {
    if (e.button === 1) {
      e.preventDefault();
      app.closeTab(t);
    }
  }
</script>

{#if app.openTabs.length}
  <nav class="tabs" aria-label="Correos abiertos">
    {#each app.openTabs as t (t.accountId + t.threadId)}
      {@const account = app.account(t.accountId)}
      <div class="tab" class:current={isCurrent(t)}>
        <button class="label" title={t.subject} onclick={() => app.select(t.accountId, t.threadId)} onauxclick={(e) => onAux(e, t)}>
          {#if account}<span class="dot" style:background={account.color}></span>{/if}
          <span class="text">{t.subject || '(sin asunto)'}</span>
        </button>
        <button class="close" title="Cerrar pestaña" onclick={() => app.closeTab(t)}><X size={13} /></button>
      </div>
    {/each}
  </nav>
{/if}

<style>
  .tabs {
    display: flex; gap: 4px; padding: 6px 10px; border-top: 1px solid var(--border); background: var(--surface);
    overflow-x: auto; flex-shrink: 0; scrollbar-width: thin;
  }
  .tab {
    display: flex; align-items: center; flex-shrink: 0; max-width: 220px; border-radius: 8px;
    background: var(--surface-2); color: var(--text-2); border: 1px solid transparent;
  }
  .tab:hover { background: var(--surface-3); color: var(--text); }
  .tab.current { background: var(--accent-soft); color: var(--accent); border-color: color-mix(in srgb, var(--accent) 35%, transparent); }
  .label {
    display: flex; align-items: center; gap: 7px; min-width: 0; padding: 5px 4px 5px 10px; border: none;
    background: none; color: inherit; font-size: 12.5px; font-weight: 500; text-align: left;
  }
  .current .label { font-weight: 600; }
  .dot { width: 7px; height: 7px; border-radius: 50%; flex-shrink: 0; }
  .text { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .close {
    display: grid; place-items: center; width: 22px; height: 22px; margin-right: 3px; border: none; border-radius: 6px;
    background: none; color: inherit; opacity: 0.6;
  }
  .close:hover { opacity: 1; background: var(--surface); }
</style>
