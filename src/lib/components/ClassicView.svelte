<script lang="ts">
  import { untrack } from 'svelte';
  import type { MessageBody } from '../api';
  import { app } from '../state.svelte';
  import { displayName, formatFullDate, formatListDate } from '../util';
  import Avatar from './Avatar.svelte';
  import MessageFrame from './MessageFrame.svelte';
  import AttachmentChips from './AttachmentChips.svelte';
  import { Reply, Sun, Moon } from '@lucide/svelte';

  let { messages, allowRemote }: { messages: MessageBody[]; allowRemote: boolean } = $props();

  let expanded = $state<Record<number, boolean>>({});
  // Momentary per-message override of the automatic light/dark rendering (not persisted).
  let themeMode = $state<Record<number, 'light' | 'dark'>>({});
  let rendersDark = $state<Record<number, boolean>>({});

  $effect(() => {
    // Expand unread messages and the latest one by default.
    const next: Record<number, boolean> = {};
    messages.forEach((m, i) => {
      if (i === messages.length - 1) next[m.summary.id] = true;
    });
    // Read without tracking: writing to state this effect depends on would loop forever.
    expanded = { ...next, ...untrack(() => expanded) };
  });

  const subject = $derived(messages[0]?.summary.subject || '(sin asunto)');
  const list = (l: { name: string | null; email: string }[]) => l.map((a) => displayName(a)).join(', ');
</script>

<div class="scroll selectable">
  <h1>{subject}</h1>
  {#each messages as m (m.summary.id)}
    {@const s = m.summary}
    {@const open = expanded[s.id]}
    <article class="card" class:open>
      <button class="head" onclick={() => (expanded[s.id] = !open)}>
        <Avatar address={s.from} size={38} />
        <div class="who">
          <div class="line">
            <span class="name">{displayName(s.from)}</span>
            {#if s.from?.name}<span class="addr">&lt;{s.from.email}&gt;</span>{/if}
          </div>
          {#if open}
            <div class="meta">
              para {list(s.to) || '(sin destinatarios)'}{#if s.cc.length} · cc {list(s.cc)}{/if}
            </div>
          {:else}
            <div class="meta snippet">{m.chatText || s.preview}</div>
          {/if}
        </div>
        <span class="date" title={formatFullDate(s.date)}>{open ? formatFullDate(s.date) : formatListDate(s.date)}</span>
      </button>
      {#if open}
        <div class="content">
          <MessageFrame html={m.html} text={m.text} {allowRemote} mode={themeMode[s.id] ?? 'auto'} bind:dark={rendersDark[s.id]} />
          <AttachmentChips message={m} />
          <div class="actions">
            <button class="btn" onclick={() => app.compose({ mode: 'reply', accountId: s.accountId, source: m })}><Reply size={15} /> Responder</button>
            {#if app.isDark}
              <button
                class="icon-btn theme"
                title={rendersDark[s.id] ? 'Ver con fondo claro' : 'Ver en modo oscuro'}
                onclick={() => (themeMode[s.id] = rendersDark[s.id] ? 'light' : 'dark')}
              >
                {#if rendersDark[s.id]}<Sun size={16} />{:else}<Moon size={16} />{/if}
              </button>
            {/if}
          </div>
        </div>
      {/if}
    </article>
  {/each}
</div>

<style>
  .scroll { flex: 1; overflow-y: auto; padding: 18px 24px 40px; }
  h1 { font-size: 21px; font-weight: 700; letter-spacing: -0.015em; margin: 4px 4px 16px; line-height: 1.3; }
  .card { background: var(--surface); border: 1px solid var(--border); border-radius: 14px; margin-bottom: 10px; box-shadow: var(--shadow); overflow: hidden; }
  .head { width: 100%; display: flex; align-items: center; gap: 12px; padding: 12px 16px; border: none; background: none; text-align: left; }
  .who { flex: 1; min-width: 0; }
  .line { display: flex; gap: 6px; align-items: baseline; flex-wrap: wrap; }
  .name { font-weight: 600; }
  .addr { color: var(--text-3); font-size: 12.5px; }
  .meta { color: var(--text-3); font-size: 12.5px; margin-top: 2px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .date { color: var(--text-3); font-size: 12px; flex-shrink: 0; }
  .content { padding: 4px 16px 16px; }
  .actions { margin-top: 14px; display: flex; gap: 8px; align-items: center; }
  .theme { margin-left: auto; }
</style>
