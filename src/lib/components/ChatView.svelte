<script lang="ts">
  // Shows a thread as an instant-messaging conversation: one bubble per email with quoted
  // history and signatures stripped, so long reply chains become readable.
  import { tick } from 'svelte';
  import { api, type Account, type MessageBody } from '../api';
  import { app } from '../state.svelte';
  import { colorFor, displayName, escapeHtml, formatDay, formatFullDate, formatTime, sameEmail, textToHtml } from '../util';
  import { linkify, replyRecipients, replySubject, signatureHtml, signatureText, threadingHeaders } from '../compose';
  import Avatar from './Avatar.svelte';
  import MessageFrame from './MessageFrame.svelte';
  import AttachmentChips from './AttachmentChips.svelte';
  import { SendHorizontal, Maximize2, ChevronDown, ChevronUp, Sun, Moon } from '@lucide/svelte';

  let { messages, account, allowRemote, onSent }: {
    messages: MessageBody[]; account: Account; allowRemote: boolean; onSent: () => void;
  } = $props();

  let scroller: HTMLElement | undefined = $state();
  let draft = $state('');
  let sending = $state(false);
  let showOriginal = $state<Record<number, boolean>>({});
  let themeMode = $state<Record<number, 'light' | 'dark'>>({});
  let rendersDark = $state<Record<number, boolean>>({});

  const subject = $derived(messages[0]?.summary.subject || '(sin asunto)');
  const last = $derived(messages[messages.length - 1]);
  const participants = $derived.by(() => {
    const seen = new Map<string, string>();
    for (const m of messages) {
      const f = m.summary.from;
      if (f && !seen.has(f.email.toLowerCase())) seen.set(f.email.toLowerCase(), displayName(f));
    }
    return [...seen.values()];
  });

  const isMine = (m: MessageBody) => !!m.summary.from && (sameEmail(m.summary.from.email, account.email) || app.isOwnAddress(m.summary.from.email));

  function newDay(i: number) {
    if (i === 0) return true;
    return new Date(messages[i - 1].summary.date * 1000).toDateString() !== new Date(messages[i].summary.date * 1000).toDateString();
  }
  // Consecutive messages from the same sender are grouped (name/avatar shown once).
  function firstOfGroup(i: number) {
    if (i === 0 || newDay(i)) return true;
    return messages[i - 1].summary.from?.email.toLowerCase() !== messages[i].summary.from?.email.toLowerCase();
  }
  function bubbleText(m: MessageBody) {
    return m.chatText || m.text || '';
  }
  function onRichClick(e: MouseEvent) {
    const a = (e.target as Element | null)?.closest?.('a');
    if (!a) return;
    e.preventDefault();
    app.openLink(a.getAttribute('href') ?? '');
  }
  function hasMore(m: MessageBody) {
    const full = (m.text ?? '').trim();
    return !!m.html || full.length > bubbleText(m).trim().length + 20;
  }

  $effect(() => {
    void messages.length;
    tick().then(() => scroller?.scrollTo({ top: scroller.scrollHeight }));
  });

  async function send() {
    const text = draft.trim();
    if (!text || !last || sending) return;
    sending = true;
    const sig = app.defaultSignature(account.id);
    const { to, cc } = replyRecipients(last, account, true);
    const { inReplyTo, references } = threadingHeaders(last);
    const sigHtml = signatureHtml(sig);
    const sigText = signatureText(sig);
    try {
      await api.sendMessage({
        accountId: account.id,
        to, cc, bcc: [],
        subject: replySubject(last),
        html: `<div>${textToHtml(text)}</div>` + (sigHtml ? `<br><div class="cornamusa-signature">-- <br>${sigHtml}</div>` : ''),
        text: text + (sigText ? `\n\n-- \n${sigText}` : ''),
        attachments: [],
        inReplyTo, references,
        forwardAttachmentsOf: null,
      });
      draft = '';
      app.toast('Enviado', 'success');
      onSent();
    } catch (e) {
      app.error(e);
    } finally {
      sending = false;
    }
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      send();
    }
  }
</script>

<div class="chat">
  <div class="chat-head">
    <div class="title">{subject}</div>
    <div class="people faint">{participants.join(', ')} · {messages.length} {messages.length === 1 ? 'mensaje' : 'mensajes'}</div>
  </div>

  <div class="scroll selectable" bind:this={scroller}>
    {#each messages as m, i (m.summary.id)}
      {@const mine = isMine(m)}
      {@const s = m.summary}
      {#if newDay(i)}
        <div class="day"><span>{formatDay(s.date)}</span></div>
      {/if}
      {#if i > 0 && s.subject && s.subject.replace(/^((re|fwd?|rv|res)\s*:\s*)+/i, '') !== messages[i - 1].summary.subject.replace(/^((re|fwd?|rv|res)\s*:\s*)+/i, '')}
        <div class="system">Asunto cambiado a «{s.subject}»</div>
      {/if}
      <div class="msg" class:mine class:grouped={!firstOfGroup(i)}>
        {#if !mine}
          <div class="avatar-slot">{#if firstOfGroup(i)}<Avatar address={s.from} size={32} />{/if}</div>
        {/if}
        <div class="bubble-col">
          {#if !mine && firstOfGroup(i)}
            <div class="sender" style:color={colorFor(s.from?.email ?? '')} title={s.from?.email}>{displayName(s.from)}</div>
          {/if}
          <div class="bubble" title={formatFullDate(s.date)}>
            {#if showOriginal[s.id]}
              <div class="original"><MessageFrame html={m.html} text={m.text} {allowRemote} mode={themeMode[s.id] ?? 'auto'} bind:dark={rendersDark[s.id]} /></div>
            {:else}
              {#if m.chatHtml}
                <!-- chatHtml comes from the backend's strict sanitizer (no style/class/id/scripts). -->
                <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
                <div class="text rich" onclick={onRichClick}>{@html m.chatHtml}</div>
              {:else}
                <div class="text">{@html linkify(escapeHtml(bubbleText(m)))}</div>
              {/if}
            {/if}
            <AttachmentChips message={m} compact />
            <div class="foot">
              {#if hasMore(m)}
                <button class="more" onclick={() => (showOriginal[s.id] = !showOriginal[s.id])}>
                  {#if showOriginal[s.id]}<ChevronUp size={13} /> Vista chat{:else}<ChevronDown size={13} /> Ver correo completo{/if}
                </button>
              {/if}
              {#if showOriginal[s.id] && app.isDark}
                <button
                  class="more"
                  title={rendersDark[s.id] ? 'Ver con fondo claro' : 'Ver en modo oscuro'}
                  onclick={() => (themeMode[s.id] = rendersDark[s.id] ? 'light' : 'dark')}
                >
                  {#if rendersDark[s.id]}<Sun size={13} />{:else}<Moon size={13} />{/if}
                </button>
              {/if}
              <span class="time">{formatTime(s.date)}</span>
            </div>
          </div>
        </div>
      </div>
    {/each}
  </div>

  <div class="composer">
    <textarea
      class="selectable"
      bind:value={draft}
      onkeydown={onKey}
      placeholder="Responder a todos… (Enter para enviar, Shift+Enter nueva línea)"
      rows="1"
      disabled={sending}
    ></textarea>
    <button class="icon-btn" title="Abrir en el editor completo" onclick={() => last && app.compose({ mode: 'replyAll', accountId: account.id, source: last })}>
      <Maximize2 size={17} />
    </button>
    <button class="send" onclick={send} disabled={!draft.trim() || sending} title="Enviar">
      <SendHorizontal size={18} />
    </button>
  </div>
</div>

<style>
  .chat { flex: 1; display: flex; flex-direction: column; min-height: 0; background: var(--chat-bg); }
  .chat-head { padding: 12px 22px; background: var(--surface); border-bottom: 1px solid var(--border); }
  .title { font-weight: 700; font-size: 16px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .people { font-size: 12.5px; margin-top: 2px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .scroll { flex: 1; overflow-y: auto; padding: 16px 22px 24px; display: flex; flex-direction: column; }
  .day { text-align: center; margin: 14px 0 8px; }
  .day span { font-size: 11.5px; font-weight: 600; color: var(--text-2); background: var(--surface); padding: 4px 12px; border-radius: 12px; box-shadow: var(--shadow); text-transform: capitalize; }
  .system { text-align: center; font-size: 12px; color: var(--text-3); margin: 6px 0; font-style: italic; }
  .msg { display: flex; gap: 8px; margin-top: 12px; max-width: 78%; align-self: flex-start; }
  .msg.grouped { margin-top: 3px; }
  .msg.mine { align-self: flex-end; flex-direction: row-reverse; }
  .avatar-slot { width: 32px; flex-shrink: 0; display: flex; align-items: flex-end; }
  .bubble-col { min-width: 0; display: flex; flex-direction: column; }
  .mine .bubble-col { align-items: flex-end; }
  .sender { font-size: 12px; font-weight: 600; margin: 0 0 3px 12px; }
  .bubble {
    background: var(--bubble-them); color: var(--text); padding: 9px 13px 6px; border-radius: 18px 18px 18px 6px;
    box-shadow: 0 1px 1.5px rgba(0, 0, 0, 0.08); min-width: 80px; max-width: 100%;
  }
  .grouped .bubble { border-top-left-radius: 6px; }
  .mine .bubble { background: var(--bubble-me); color: var(--bubble-me-text); border-radius: 18px 18px 6px 18px; }
  .mine.grouped .bubble { border-top-right-radius: 6px; }
  .text { white-space: pre-wrap; overflow-wrap: anywhere; line-height: 1.45; }
  .text :global(a) { color: inherit; text-decoration: underline; }
  /* Formatted bubble: structure from the email, colors from the app theme. */
  .rich { white-space: normal; overflow-x: auto; }
  .rich :global(p), .rich :global(h1), .rich :global(h2), .rich :global(h3), .rich :global(h4) { margin: 0 0 0.6em; }
  .rich :global(h1), .rich :global(h2), .rich :global(h3), .rich :global(h4) { font-size: 1.05em; }
  .rich :global(p:empty) { display: none; }
  .rich :global(ul), .rich :global(ol) { margin: 0.3em 0 0.6em; padding-left: 1.5em; }
  .rich :global(li) { margin: 0.15em 0; }
  .rich :global(blockquote) { margin: 0.4em 0; padding-left: 0.8em; border-left: 2px solid color-mix(in srgb, currentColor 35%, transparent); opacity: 0.85; }
  .rich :global(pre) { white-space: pre-wrap; font-size: 0.92em; }
  .rich :global(table) { border-collapse: collapse; margin: 0.4em 0 0.7em; font-size: 0.95em; }
  .rich :global(td), .rich :global(th) { border: 1px solid color-mix(in srgb, currentColor 22%, transparent); padding: 4px 8px; vertical-align: top; text-align: left; }
  .rich :global(th) { font-weight: 600; }
  .rich :global(img) { max-width: 100%; height: auto; }
  .rich :global(hr) { border: none; border-top: 1px solid color-mix(in srgb, currentColor 25%, transparent); }
  .rich > :global(:last-child) { margin-bottom: 0; }
  .original { background: var(--surface); color: var(--text); border-radius: 10px; padding: 8px; margin: 2px -6px 0; min-width: min(560px, 60vw); }
  .foot { display: flex; align-items: center; justify-content: flex-end; gap: 10px; margin-top: 3px; }
  .time { font-size: 11px; opacity: 0.6; }
  .more { border: none; background: none; color: inherit; opacity: 0.75; font-size: 11.5px; display: flex; align-items: center; gap: 3px; padding: 0; margin-right: auto; }
  .more:hover { opacity: 1; text-decoration: underline; }
  .composer { display: flex; align-items: flex-end; gap: 8px; padding: 10px 16px 14px; background: var(--surface); border-top: 1px solid var(--border); }
  textarea {
    flex: 1; resize: none; border: 1px solid var(--border); background: var(--surface-2); border-radius: 20px;
    padding: 9px 16px; outline: none; max-height: 160px; field-sizing: content; line-height: 1.4;
  }
  textarea:focus { border-color: var(--accent); background: var(--surface); }
  .send { width: 40px; height: 40px; border-radius: 50%; border: none; background: var(--accent); color: #fff; display: grid; place-items: center; flex-shrink: 0; }
  .send:disabled { opacity: 0.4; cursor: default; }
</style>
