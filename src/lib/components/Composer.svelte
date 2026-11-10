<script lang="ts">
  import { onDestroy, onMount, untrack } from 'svelte';
  import { getCurrentWebview } from '@tauri-apps/api/webview';
  import { api, type EmailAddress, type OutgoingMessage, type PickedFile } from '../api';
  import { app, type ComposeInit } from '../state.svelte';
  import { formatSize, isSendShortcut, isValidEmail, sendShortcutLabel } from '../util';
  import { initialDraft, signatureHtml, signatureText } from '../compose';
  import RichEditor from './RichEditor.svelte';
  import RecipientInput from './RecipientInput.svelte';
  import MessageFrame from './MessageFrame.svelte';
  import { X, Paperclip, SendHorizontal, Trash2, Minus, Maximize2, Minimize2, FileText } from '@lucide/svelte';

  let { init }: { init: ComposeInit } = $props();

  // The composer is re-created (keyed) for every new init, so reading it once is intended.
  const init0 = untrack(() => init);
  const src = init0.source;
  const start = initialDraft(init0, app.account(init0.accountId ?? src?.summary.accountId ?? ''), app.accounts[0]?.id ?? '');
  const mode = init0.mode;
  const sigKind = mode === 'new' ? 'new' : 'reply';

  let accountId = $state(start.accountId);
  let to = $state<EmailAddress[]>(start.to);
  let cc = $state<EmailAddress[]>(start.cc);
  let bcc = $state<EmailAddress[]>([]);
  let showCc = $state(start.cc.length > 0);
  let subject = $state(start.subject);
  let bodyHtml = $state(start.content);
  let bodyText = $state(start.contentText);
  let attachments = $state<PickedFile[]>([]);
  let signatureId = $state<string>(mode === 'draft' ? '' : (app.defaultSignature(start.accountId, sigKind)?.id ?? ''));
  let includeQuote = $state(true);
  const { quoteHtml, quoteText, inReplyTo, references, forwardOf } = start;
  let draftId = $state<string | null>(start.draftId);
  let sending = $state(false);
  let saving = $state(false);
  let minimized = $state(false);
  let maximized = $state(false);
  let dragOver = $state(false);

  // A draft is dirty when it differs from what was last saved (or from how it was opened).
  const snapshot = $derived(
    JSON.stringify([accountId, to, cc, bcc, subject, bodyHtml, attachments.map((a) => a.path), signatureId, includeQuote]),
  );
  let savedSnapshot = $state(untrack(() => snapshot));
  const dirty = $derived(snapshot !== savedSnapshot);

  const account = $derived(app.account(accountId));
  const signature = $derived(app.signatures.find((s) => s.id === signatureId));
  const availableSignatures = $derived(app.signaturesFor(accountId));
  const sendKey = $derived(app.settings?.composerSendKey ?? 'ctrlEnter');
  const totalSize = $derived(attachments.reduce((n, a) => n + a.size, 0));
  const title = $derived(subject.trim() || (mode === 'forward' ? 'Reenviar' : mode.startsWith('reply') ? 'Responder' : 'Mensaje nuevo'));

  // Changing the "From" account switches to that account's default signature.
  let lastAccount = start.accountId;
  $effect(() => {
    if (accountId !== lastAccount) {
      lastAccount = accountId;
      signatureId = app.defaultSignature(accountId, sigKind)?.id ?? '';
    }
  });

  function buildMessage(): OutgoingMessage {
    const sigHtml = signatureHtml(signature);
    const sigText = signatureText(signature);
    const quote = includeQuote && quoteHtml;
    return {
      accountId,
      to, cc, bcc,
      subject: subject.trim(),
      html:
        bodyHtml +
        (sigHtml ? `<br><div class="cornamusa-signature">-- <br>${sigHtml}</div>` : '') +
        (quote ? `<br>${quoteHtml}` : ''),
      text: bodyText + (sigText ? `\n\n-- \n${sigText}` : '') + (quote ? `\n\n${quoteText}` : ''),
      attachments: attachments.map((a) => ({ path: a.path })),
      inReplyTo,
      references,
      forwardAttachmentsOf: forwardOf,
    };
  }

  function hasContent() {
    return bodyText.trim().length > 0 || to.length > 0 || subject.trim().length > 0 || attachments.length > 0;
  }

  async function saveDraft(silent = true) {
    if (!dirty || !hasContent() || saving || sending || !accountId) return;
    saving = true;
    try {
      const snap = snapshot;
      draftId = await api.saveDraft(buildMessage(), draftId);
      savedSnapshot = snap;
      if (!silent) app.toast('Guardado en Borradores', 'success');
    } catch (e) {
      if (!silent) app.error(e);
    } finally {
      saving = false;
    }
  }

  async function send() {
    if (sending) return;
    const all = [...to, ...cc, ...bcc];
    if (all.length === 0) return app.toast('Agrega al menos un destinatario', 'error');
    const bad = all.find((a) => !isValidEmail(a.email));
    if (bad) return app.toast(`Dirección inválida: ${bad.email}`, 'error');
    if (!subject.trim() && !confirm('¿Enviar sin asunto?')) return;
    if (/\badjunt[oa]s?\b|\battach(ed|ment)\b/i.test(bodyText) && attachments.length === 0 && !forwardOf &&
      !confirm('El mensaje menciona un adjunto pero no adjuntaste nada. ¿Enviar de todos modos?')) return;
    sending = true;
    try {
      await api.sendMessage(buildMessage());
      if (draftId) api.deleteDraft(accountId, draftId).catch(() => {});
      app.toast('Mensaje enviado', 'success');
      app.composer = null;
    } catch (e) {
      app.error(e);
    } finally {
      sending = false;
    }
  }

  async function close() {
    if (dirty && hasContent()) await saveDraft(false);
    app.composer = null;
  }

  async function discard() {
    if (hasContent() && !confirm('¿Descartar este mensaje?')) return;
    if (draftId) await api.deleteDraft(accountId, draftId).catch(() => {});
    app.composer = null;
  }

  async function pickFiles() {
    try {
      add(await api.pickAttachments());
    } catch (e) {
      app.error(e);
    }
  }

  function add(files: PickedFile[]) {
    for (const f of files) if (!attachments.some((a) => a.path === f.path)) attachments = [...attachments, f];
  }

  function onKey(e: KeyboardEvent) {
    if (isSendShortcut(e, sendKey)) {
      e.preventDefault();
      send();
    } else if (e.key === 'Escape') {
      e.preventDefault();
      e.stopPropagation();
      close();
    }
  }

  let timer: ReturnType<typeof setInterval>;
  let unlistenDrop: (() => void) | undefined;

  onMount(async () => {
    timer = setInterval(() => saveDraft(), 20_000);
    // Tauri delivers dropped files with their real paths.
    unlistenDrop = await getCurrentWebview().onDragDropEvent(async (event) => {
      const p = event.payload;
      if (p.type === 'over' || p.type === 'enter') dragOver = true;
      else if (p.type === 'leave') dragOver = false;
      else if (p.type === 'drop') {
        dragOver = false;
        add(await api.inspectFiles(p.paths));
      }
    });
  });

  onDestroy(() => {
    clearInterval(timer);
    unlistenDrop?.();
  });

</script>

<div class="composer" class:minimized class:maximized role="dialog" aria-label={title} tabindex="-1" onkeydown={onKey}>
  <header>
    <button class="title" onclick={() => (minimized = !minimized)}>{title}</button>
    {#if saving}<span class="faint status">Guardando…</span>{:else if draftId && !dirty}<span class="faint status">Borrador guardado</span>{/if}
    <button class="icon-btn" title="Minimizar" onclick={() => (minimized = !minimized)}><Minus size={16} /></button>
    <button class="icon-btn" title={maximized ? 'Restaurar' : 'Maximizar'} onclick={() => { maximized = !maximized; minimized = false; }}>
      {#if maximized}<Minimize2 size={15} />{:else}<Maximize2 size={15} />{/if}
    </button>
    <button class="icon-btn" title="Cerrar (guarda borrador)" onclick={close}><X size={17} /></button>
  </header>

  {#if !minimized}
    <div class="fields">
      {#if app.accounts.length > 1}
        <div class="row">
          <span class="label">De</span>
          <select bind:value={accountId} class="from">
            {#each app.accounts as a (a.id)}<option value={a.id}>{a.displayName} &lt;{a.email}&gt;</option>{/each}
          </select>
        </div>
      {:else if account}
        <div class="row"><span class="label">De</span><span class="muted">{account.displayName} &lt;{account.email}&gt;</span></div>
      {/if}
      <div class="to-row">
        <RecipientInput label="Para" bind:value={to} autofocus={mode === 'new' || mode === 'forward'} />
        {#if !showCc}<button class="cc-toggle" onclick={() => (showCc = true)}>CC / CCO</button>{/if}
      </div>
      {#if showCc}
        <RecipientInput label="CC" bind:value={cc} />
        <RecipientInput label="CCO" bind:value={bcc} />
      {/if}
      <div class="row">
        <span class="label">Asunto</span>
        <input class="subject selectable" bind:value={subject} placeholder="Asunto" />
      </div>
    </div>

    <div class="body" class:drag={dragOver}>
      <RichEditor content={start.content} onchange={(h, t) => { bodyHtml = h; bodyText = t; }} />

      {#if signature}
        <div class="signature">
          <span class="faint">-- </span>
          {#if signature.isHtml}<MessageFrame html={signature.body} allowRemote />{:else}<div class="sig-text">{signature.body}</div>{/if}
        </div>
      {/if}

      {#if quoteHtml}
        <div class="quote">
          <label class="faint"><input type="checkbox" bind:checked={includeQuote} /> Incluir {mode === 'forward' ? 'mensaje reenviado' : 'texto citado'}</label>
          {#if includeQuote}<div class="quote-preview"><MessageFrame html={quoteHtml} /></div>{/if}
        </div>
      {/if}

      {#if dragOver}<div class="drop">Suelta los archivos para adjuntarlos</div>{/if}
    </div>

    {#if attachments.length || forwardOf}
      <div class="attachments">
        {#if forwardOf && src}
          {#each src.attachments.filter((a) => !a.inline) as a (a.index)}
            <span class="att"><FileText size={14} /> {a.filename} <span class="faint">{formatSize(a.size)}</span></span>
          {/each}
        {/if}
        {#each attachments as a (a.path)}
          <span class="att" title={a.path}>
            <Paperclip size={14} /> {a.name} <span class="faint">{formatSize(a.size)}</span>
            <button aria-label="Quitar" onclick={() => (attachments = attachments.filter((x) => x.path !== a.path))}><X size={13} /></button>
          </span>
        {/each}
        {#if totalSize > 25 * 1024 * 1024}
          <span class="warn">Los adjuntos superan 25 MB; Gmail y Outlook podrían rechazar el mensaje.</span>
        {/if}
      </div>
    {/if}

    <footer>
      <button class="btn primary send" onclick={send} disabled={sending}>
        <SendHorizontal size={16} /> {sending ? 'Enviando…' : 'Enviar'}
      </button>
      {#if sendShortcutLabel(sendKey)}<span class="faint hint">{#each sendShortcutLabel(sendKey)!.split('+') as k, i}{#if i}+{/if}<kbd>{k}</kbd>{/each}</span>{/if}
      <button class="icon-btn" title="Adjuntar archivos" onclick={pickFiles}><Paperclip size={18} /></button>
      <select class="sig" bind:value={signatureId} title="Firma">
        <option value="">Sin firma</option>
        {#each availableSignatures as s (s.id)}<option value={s.id}>✍ {s.name}</option>{/each}
      </select>
      <span class="grow"></span>
      <button class="icon-btn" title="Descartar" onclick={discard}><Trash2 size={17} /></button>
    </footer>
  {/if}
</div>

<style>
  .composer {
    position: fixed; right: 24px; bottom: 0; width: 680px; height: min(720px, calc(100vh - 40px)); z-index: 30;
    background: var(--surface); border: 1px solid var(--border); border-bottom: none; border-radius: 14px 14px 0 0;
    box-shadow: var(--shadow-lg); display: flex; flex-direction: column; animation: pop 0.15s ease-out; outline: none;
  }
  .composer.maximized { inset: 24px 48px; width: auto; height: auto; border-radius: 14px; border-bottom: 1px solid var(--border); }
  .composer.minimized { height: auto; width: 360px; }
  header { display: flex; align-items: center; gap: 4px; padding: 6px 8px 6px 16px; background: var(--surface-2); border-radius: 14px 14px 0 0; }
  .title { flex: 1; text-align: left; border: none; background: none; font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; padding: 4px 0; }
  .status { font-size: 12px; margin-right: 6px; }
  .fields { flex-shrink: 0; }
  .row { display: flex; align-items: center; gap: 10px; padding: 0 18px; border-bottom: 1px solid var(--border); min-height: 42px; }
  .label { color: var(--text-3); width: 46px; flex-shrink: 0; font-size: 13px; }
  .from { flex: 1; border: none; background: transparent; outline: none; padding: 8px 0; }
  .subject { flex: 1; border: none; outline: none; background: transparent; padding: 10px 0; font-weight: 500; }
  .to-row { position: relative; }
  .cc-toggle { position: absolute; right: 14px; top: 10px; border: none; background: none; color: var(--text-3); font-size: 12.5px; }
  .cc-toggle:hover { color: var(--accent); }
  .body { flex: 1; overflow-y: auto; position: relative; display: flex; flex-direction: column; }
  .body.drag { outline: 2px dashed var(--accent); outline-offset: -8px; }
  .drop { position: absolute; inset: 0; display: grid; place-items: center; background: color-mix(in srgb, var(--accent) 10%, transparent); color: var(--accent); font-weight: 600; pointer-events: none; }
  /* Nothing in the body may shrink: the editor would collapse and the signature and quote
     below it would be drawn over its toolbar and text. The body scrolls instead. */
  .body > * { flex-shrink: 0; }
  .body > :global(.editor) { flex: 1 0 auto; }
  .signature { margin: 0 18px 10px; padding-top: 8px; border-top: 1px dashed var(--border); }
  .sig-text { white-space: pre-wrap; color: var(--text-2); }
  .quote { padding: 4px 18px 14px; }
  .quote label { font-size: 12.5px; display: flex; gap: 6px; align-items: center; }
  .quote-preview { margin-top: 6px; border-left: 3px solid var(--border); padding-left: 10px; opacity: 0.85; max-height: 260px; overflow: auto; }
  .attachments { display: flex; flex-wrap: wrap; gap: 6px; padding: 8px 18px; border-top: 1px solid var(--border); }
  .att { display: inline-flex; align-items: center; gap: 6px; padding: 4px 6px 4px 10px; background: var(--surface-2); border-radius: 8px; font-size: 12.5px; }
  .att button { border: none; background: none; color: var(--text-3); display: grid; padding: 2px; }
  .warn { color: var(--warning); font-size: 12px; }
  footer { display: flex; align-items: center; gap: 8px; padding: 10px 14px; border-top: 1px solid var(--border); }
  .send { padding: 8px 18px; border-radius: 20px; }
  .hint { font-size: 11px; }
  .sig { border: 1px solid var(--border); background: var(--surface); border-radius: 8px; padding: 5px 8px; font-size: 12.5px; max-width: 180px; }
  .grow { flex: 1; }
</style>
