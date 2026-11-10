<script lang="ts">
  // Signatures: plain text, or HTML edited either visually (rich editor) or as source code.
  import { app } from '../state.svelte';
  import { api, type Signature } from '../api';
  import RichEditor from './RichEditor.svelte';
  import MessageFrame from './MessageFrame.svelte';
  import { Plus, Trash2, Code, Eye, Type } from '@lucide/svelte';

  let current = $state<Signature | null>(null);
  let htmlMode = $state<'visual' | 'code'>('visual');
  let editorKey = $state(0);

  // The visual editor (TipTap) has no tables and drops inline styles, so a typical designed
  // signature (logo and details side by side) would be flattened there. Open those as code.
  const hasLayout = (html: string) => /<table\b/i.test(html);
  const lossy = $derived(!!current?.isHtml && hasLayout(current.body));

  function edit(s: Signature) {
    current = { ...s };
    htmlMode = s.isHtml && hasLayout(s.body) ? 'code' : 'visual';
    editorKey++;
  }

  function create() {
    current = { id: '', accountId: null, name: 'Nueva firma', isHtml: true, body: '', isDefault: app.signatures.length === 0 };
    htmlMode = 'visual';
    editorKey++;
  }

  async function save() {
    if (!current) return;
    try {
      current = await api.saveSignature(current);
      await app.reloadSignatures();
      editorKey++;
      app.toast('Firma guardada', 'success');
    } catch (e) {
      app.error(e);
    }
  }

  async function remove(s: Signature) {
    if (!confirm(`¿Eliminar la firma «${s.name}»?`)) return;
    await api.deleteSignature(s.id).catch((e) => app.error(e));
    await app.reloadSignatures();
    if (current?.id === s.id) current = null;
  }

  function setMode(isHtml: boolean) {
    if (!current || current.isHtml === isHtml) return;
    if (isHtml) {
      current.body = current.body.split('\n').map((l) => l.replace(/&/g, '&amp;').replace(/</g, '&lt;')).join('<br>');
    } else {
      const doc = new DOMParser().parseFromString(current.body, 'text/html');
      doc.querySelectorAll('br').forEach((b) => b.replaceWith('\n'));
      doc.querySelectorAll('p,div').forEach((b) => b.append('\n'));
      current.body = (doc.body.textContent ?? '').trim();
    }
    current.isHtml = isHtml;
    editorKey++;
  }

  const KINDS = [['new', 'Mensajes nuevos'], ['reply', 'Respuestas y reenvíos']] as const;

  /** Where a signature is used by default, for its subtitle in the list. */
  function usage(s: Signature): string {
    const kinds = new Set<string>();
    for (const a of app.accounts) {
      if (app.signatureDefaultId(a.id, 'new') === s.id) kinds.add('nuevos');
      if (app.signatureDefaultId(a.id, 'reply') === s.id) kinds.add('respuestas');
    }
    return kinds.size ? ` · ${[...kinds].join(' y ')}` : '';
  }

  function switchHtmlMode(m: 'visual' | 'code') {
    htmlMode = m;
    editorKey++;
  }
</script>

{#if app.signatures.length && app.accounts.length}
  <div class="defaults">
    <h4>Firma predeterminada</h4>
    <p class="faint small">Se pone sola al redactar; puedes cambiarla en cada mensaje.</p>
    <table>
      <thead><tr><th></th>{#each KINDS as [, label]}<th>{label}</th>{/each}</tr></thead>
      <tbody>
        {#each app.accounts as a (a.id)}
          <tr>
            <td class="acct" title={a.email}>{a.email}</td>
            {#each KINDS as [kind, label]}
              <td>
                <select class="input" aria-label="{label} · {a.email}" value={app.signatureDefaultId(a.id, kind) ?? ''}
                  onchange={(e) => app.setSignatureDefault(a.id, kind, e.currentTarget.value || null)}>
                  <option value="">Sin firma</option>
                  {#each app.signaturesFor(a.id) as sig (sig.id)}<option value={sig.id}>{sig.name}</option>{/each}
                </select>
              </td>
            {/each}
          </tr>
        {/each}
      </tbody>
    </table>
  </div>
{/if}

<div class="sig">
  <aside>
    {#each app.signatures as s (s.id)}
      <div class="item" class:active={current?.id === s.id}>
        <button class="pick" onclick={() => edit(s)}>
          <strong>{s.name}</strong>
          <span class="faint">{s.accountId ? (app.account(s.accountId)?.email ?? '') : 'Todas las cuentas'}{usage(s)}</span>
        </button>
        <button class="icon-btn" title="Eliminar" onclick={() => remove(s)}><Trash2 size={15} /></button>
      </div>
    {/each}
    <button class="btn" onclick={create}><Plus size={15} /> Nueva firma</button>
  </aside>

  {#if current}
    <div class="edit">
      <div class="grid">
        <label class="field">Nombre<input class="input" bind:value={current.name} /></label>
        <label class="field">Cuenta
          <select class="input" bind:value={current.accountId}>
            <option value={null}>Todas las cuentas</option>
            {#each app.accounts as a (a.id)}<option value={a.id}>{a.email}</option>{/each}
          </select>
        </label>
      </div>

      <div class="modes">
        <div class="seg">
          <button class:on={!current.isHtml} onclick={() => setMode(false)}><Type size={14} /> Texto</button>
          <button class:on={current.isHtml} onclick={() => setMode(true)}><Code size={14} /> HTML</button>
        </div>
        {#if current.isHtml}
          <div class="seg">
            <button class:on={htmlMode === 'visual'} onclick={() => switchHtmlMode('visual')}><Eye size={14} /> Visual</button>
            <button class:on={htmlMode === 'code'} onclick={() => switchHtmlMode('code')}><Code size={14} /> Código HTML</button>
          </div>
        {/if}
      </div>

      {#key editorKey}
        {#if !current.isHtml}
          <textarea class="input code plain selectable" bind:value={current.body} rows="8" placeholder={'Juan Pérez\nGerente de Ventas · ACME\n+56 9 1234 5678'}></textarea>
        {:else if htmlMode === 'visual'}
          {#if lossy}
            <p class="warn">Esta firma usa tablas, que el editor visual no puede mostrar: si la editas aquí se perderá su diseño. Para verla y editarla tal cual, usa <button class="link" onclick={() => switchHtmlMode('code')}>Código HTML</button>.</p>
          {/if}
          <div class="box"><RichEditor content={current.body} minHeight={160} fitWidth={false} placeholder="Diseña tu firma…" onchange={(h) => current && (current.body = h)} /></div>
        {:else}
          <textarea class="input code selectable" bind:value={current.body} rows="10" spellcheck="false"
            placeholder={'<table><tr><td><img src="https://…/logo.png" width="64"></td><td><b>Juan Pérez</b><br>ACME</td></tr></table>'}></textarea>
          <div class="preview-label faint">Vista previa</div>
          <div class="box preview"><MessageFrame html={current.body} allowRemote mode="light" fit={false} /></div>
        {/if}
      {/key}
      <p class="faint small">El HTML se sanitiza al guardar: se eliminan scripts, formularios y atributos peligrosos.</p>
      <div><button class="btn primary" onclick={save}>Guardar firma</button></div>
    </div>
  {:else}
    <div class="empty faint">Selecciona o crea una firma</div>
  {/if}
</div>

<style>
  .defaults { border: 1px solid var(--border); border-radius: 12px; padding: 12px 16px; margin-bottom: 18px; }
  .defaults h4 { margin: 0; font-size: 14px; }
  .defaults p { margin: 2px 0 8px; }
  .defaults table { width: 100%; border-collapse: collapse; }
  .defaults th { text-align: left; font-size: 12px; font-weight: 600; color: var(--text-3); padding: 2px 6px; }
  .defaults td { padding: 3px 6px; }
  .defaults td.acct { font-size: 13px; max-width: 260px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .defaults select { width: 100%; padding: 5px 8px; font-size: 13px; }
  .sig { display: flex; gap: 20px; min-height: 420px; }
  aside { width: 180px; flex-shrink: 0; display: flex; flex-direction: column; gap: 6px; }
  .item { display: flex; align-items: center; border-radius: 10px; border: 1px solid var(--border); }
  .item.active { border-color: var(--accent); background: var(--accent-soft); }
  .pick { flex: 1; min-width: 0; display: flex; flex-direction: column; align-items: flex-start; gap: 2px; padding: 8px 10px; border: none; background: none; text-align: left; }
  .pick span { font-size: 11.5px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: 100%; }
  .edit { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 12px; }
  .grid { display: grid; grid-template-columns: 1fr 1fr; gap: 12px; }
  .modes { display: flex; gap: 10px; }
  .seg { display: flex; background: var(--surface-2); padding: 3px; border-radius: 9px; }
  .seg button { display: flex; gap: 5px; align-items: center; border: none; background: none; padding: 5px 10px; border-radius: 7px; font-size: 12.5px; color: var(--text-2); }
  .seg button.on { background: var(--surface); color: var(--accent); font-weight: 600; box-shadow: var(--shadow); }
  .code { font-family: ui-monospace, 'Cascadia Code', monospace; font-size: 12.5px; resize: vertical; }
  .code.plain { font-family: inherit; font-size: 14px; }
  .box { border: 1px solid var(--border); border-radius: 10px; overflow: hidden; }
  .preview { padding: 10px; background: #fff; }
  .preview-label { font-size: 12px; margin-bottom: -6px; }
  .small { font-size: 12px; margin: 0; }
  .warn { margin: 0; font-size: 12.5px; padding: 8px 12px; border-radius: 9px; background: color-mix(in srgb, #e0a100 14%, transparent); color: var(--text); }
  .link { border: none; background: none; padding: 0; color: var(--accent); font: inherit; text-decoration: underline; cursor: pointer; }
  .empty { flex: 1; display: grid; place-items: center; }
</style>
