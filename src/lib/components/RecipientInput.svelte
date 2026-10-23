<script lang="ts">
  import { api, type EmailAddress } from '../api';
  import { displayName, isValidEmail, parseAddresses } from '../util';
  import { X } from '@lucide/svelte';

  let { label, value = $bindable([]), autofocus = false }: { label: string; value?: EmailAddress[]; autofocus?: boolean } = $props();

  let text = $state('');
  let suggestions = $state<EmailAddress[]>([]);
  let active = $state(0);
  let input: HTMLInputElement | undefined = $state();
  let timer: ReturnType<typeof setTimeout> | undefined;

  $effect(() => {
    if (autofocus) input?.focus();
  });

  function commit(raw = text) {
    const parsed = parseAddresses(raw).filter((a) => a.email);
    for (const a of parsed) {
      if (!value.some((v) => v.email.toLowerCase() === a.email.toLowerCase())) value = [...value, a];
    }
    text = '';
    suggestions = [];
  }

  function pick(a: EmailAddress) {
    if (!value.some((v) => v.email.toLowerCase() === a.email.toLowerCase())) value = [...value, a];
    text = '';
    suggestions = [];
    input?.focus();
  }

  function onInput() {
    if (/[,;]/.test(text)) return commit();
    clearTimeout(timer);
    const q = text.trim();
    if (q.length < 2) {
      suggestions = [];
      return;
    }
    timer = setTimeout(async () => {
      suggestions = (await api.contacts(q).catch(() => [])).filter((s) => !value.some((v) => v.email === s.email));
      active = 0;
    }, 150);
  }

  function onKey(e: KeyboardEvent) {
    if (suggestions.length && (e.key === 'ArrowDown' || e.key === 'ArrowUp')) {
      e.preventDefault();
      active = (active + (e.key === 'ArrowDown' ? 1 : suggestions.length - 1)) % suggestions.length;
    } else if (e.key === 'Enter' || e.key === 'Tab') {
      if (suggestions.length) {
        e.preventDefault();
        pick(suggestions[active]);
      } else if (text.trim()) {
        e.preventDefault();
        commit();
      }
    } else if (e.key === 'Backspace' && !text && value.length) {
      value = value.slice(0, -1);
    } else if (e.key === 'Escape' && suggestions.length) {
      e.stopPropagation();
      suggestions = [];
    }
  }

  function onPaste(e: ClipboardEvent) {
    const data = e.clipboardData?.getData('text') ?? '';
    if (/[,;\n]/.test(data)) {
      e.preventDefault();
      commit(data);
    }
  }
</script>

<div class="field" role="presentation" onclick={() => input?.focus()}>
  <span class="label">{label}</span>
  <div class="chips">
    {#each value as a, i (a.email + i)}
      <span class="chip" class:invalid={!isValidEmail(a.email)} title={a.email}>
        {displayName(a)}
        <button type="button" aria-label="Quitar" onclick={() => (value = value.filter((_, j) => j !== i))}><X size={12} /></button>
      </span>
    {/each}
    <input
      bind:this={input}
      bind:value={text}
      oninput={onInput}
      onkeydown={onKey}
      onpaste={onPaste}
      onblur={() => setTimeout(() => text.trim() && commit(), 150)}
      class="selectable"
    />
  </div>
  {#if suggestions.length}
    <ul class="suggest">
      {#each suggestions as s, i (s.email)}
        <li>
          <button type="button" class:active={i === active} onmousedown={(e) => { e.preventDefault(); pick(s); }}>
            <strong>{s.name ?? s.email}</strong>{#if s.name}<span>{s.email}</span>{/if}
          </button>
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .field { position: relative; display: flex; align-items: center; gap: 10px; padding: 6px 18px; border-bottom: 1px solid var(--border); min-height: 42px; cursor: text; }
  .label { color: var(--text-3); width: 46px; flex-shrink: 0; font-size: 13px; }
  .chips { flex: 1; display: flex; flex-wrap: wrap; gap: 5px; align-items: center; }
  .chip { display: inline-flex; align-items: center; gap: 4px; background: var(--accent-soft); color: var(--accent); padding: 3px 4px 3px 10px; border-radius: 14px; font-size: 13px; font-weight: 500; }
  .chip.invalid { background: color-mix(in srgb, var(--danger) 15%, transparent); color: var(--danger); }
  .chip button { border: none; background: none; color: inherit; display: grid; place-items: center; padding: 2px; border-radius: 50%; }
  .chip button:hover { background: rgba(0, 0, 0, 0.08); }
  input { flex: 1; min-width: 140px; border: none; outline: none; background: transparent; padding: 4px 0; }
  .suggest { position: absolute; left: 70px; top: 100%; z-index: 10; list-style: none; margin: 2px 0 0; padding: 4px; min-width: 320px; background: var(--surface); border: 1px solid var(--border); border-radius: 10px; box-shadow: var(--shadow-lg); }
  .suggest button { width: 100%; text-align: left; display: flex; flex-direction: column; border: none; background: none; padding: 7px 10px; border-radius: 7px; }
  .suggest button.active, .suggest button:hover { background: var(--accent-soft); }
  .suggest span { color: var(--text-3); font-size: 12px; }
</style>
