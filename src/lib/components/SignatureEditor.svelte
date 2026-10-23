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

  function edit(s: Signature) {
    current = { ...s };
    htmlMode = 'visual';
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

  function switchHtmlMode(m: 'visual' | 'code') {
    htmlMode = m;
    editorKey++;
  }
</script>

<div class="sig">
  <aside>
    {#each app.signatures as s (s.id)}
      <div class="item" class:active={current?.id === s.id}>
        <button class="pick" onclick={() => edit(s)}>
          <strong>{s.name}</strong>
          <span class="faint">{s.accountId ? (app.account(s.accountId)?.email ?? '') : 'Todas las cuentas'}{s.isDefault ? ' · predeterminada' : ''}</span>
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
      <label class="check"><input type="checkbox" bind:checked={current.isDefault} /> Usar por defecto al redactar</label>

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
          <div class="box"><RichEditor content={current.body} minHeight={160} placeholder="Diseña tu firma…" onchange={(h) => current && (current.body = h)} /></div>
        {:else}
          <textarea class="input code selectable" bind:value={current.body} rows="10" spellcheck="false"
            placeholder={'<table><tr><td><img src="https://…/logo.png" width="64"></td><td><b>Juan Pérez</b><br>ACME</td></tr></table>'}></textarea>
          <div class="preview-label faint">Vista previa</div>
          <div class="box preview"><MessageFrame html={current.body} allowRemote /></div>
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
  .sig { display: flex; gap: 20px; min-height: 420px; }
  aside { width: 220px; display: flex; flex-direction: column; gap: 6px; }
  .item { display: flex; align-items: center; border-radius: 10px; border: 1px solid var(--border); }
  .item.active { border-color: var(--accent); background: var(--accent-soft); }
  .pick { flex: 1; min-width: 0; display: flex; flex-direction: column; align-items: flex-start; gap: 2px; padding: 8px 10px; border: none; background: none; text-align: left; }
  .pick span { font-size: 11.5px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: 100%; }
  .edit { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 12px; }
  .grid { display: grid; grid-template-columns: 1fr 1fr; gap: 12px; }
  .check { display: flex; gap: 8px; align-items: center; font-size: 13px; }
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
  .empty { flex: 1; display: grid; place-items: center; }
</style>
