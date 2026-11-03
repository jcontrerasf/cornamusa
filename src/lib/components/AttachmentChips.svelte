<script lang="ts">
  import type { AttachmentInfo, MessageBody } from '../api';
  import { app } from '../state.svelte';
  import { formatSize } from '../util';
  import { Paperclip, FileImage, FileText, FileArchive, MoreHorizontal, ExternalLink, AppWindow, Download, ShieldAlert } from '@lucide/svelte';

  let { message, compact = false }: { message: MessageBody; compact?: boolean } = $props();
  const files = $derived(message.attachments.filter((a) => !a.inline));
  let menuFor = $state<number | null>(null);
  // The menu is position:fixed so cards with overflow:hidden don't clip it.
  let menuPos = $state({ top: 0, left: 0 });
  const MENU_W = 270;
  const MENU_H = 130;

  // A fixed menu would drift away from its chip when anything scrolls: close it instead.
  $effect(() => {
    if (menuFor === null) return;
    const close = () => (menuFor = null);
    window.addEventListener('scroll', close, true);
    window.addEventListener('resize', close);
    return () => {
      window.removeEventListener('scroll', close, true);
      window.removeEventListener('resize', close);
    };
  });

  function toggleMenu(e: MouseEvent, a: AttachmentInfo) {
    e.stopPropagation();
    if (menuFor === a.index) {
      menuFor = null;
      return;
    }
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const below = r.bottom + 4 + MENU_H < window.innerHeight;
    menuPos = {
      top: below ? r.bottom + 4 : r.top - 4 - MENU_H,
      left: Math.max(8, Math.min(window.innerWidth - MENU_W - 8, r.right - MENU_W)),
    };
    menuFor = a.index;
  }

  function icon(a: AttachmentInfo) {
    if (a.kind === 'saveOnly') return ShieldAlert;
    if (a.mimeType.startsWith('image/')) return FileImage;
    if (a.mimeType.includes('zip') || a.mimeType.includes('compressed') || a.mimeType.includes('tar')) return FileArchive;
    if (a.kind === 'pdf' || a.mimeType.startsWith('text/') || a.mimeType.includes('document')) return FileText;
    return Paperclip;
  }

  function tip(a: AttachmentInfo) {
    if (a.kind === 'image' || a.kind === 'pdf') return `Ver ${a.filename}`;
    if (a.kind === 'external') return `Abrir ${a.filename}`;
    return `Por seguridad este tipo de archivo solo se puede guardar (${a.filename})`;
  }

  function run(fn: () => void) {
    menuFor = null;
    fn();
  }
</script>

<svelte:window onclick={() => (menuFor = null)} />

{#if files.length}
  <div class="chips" class:compact>
    {#each files as a (a.index)}
      {@const Icon = icon(a)}
      <div class="chip" class:risky={a.kind === 'saveOnly'}>
        <button class="main" onclick={() => app.openAttachment(message, a)} title={tip(a)}>
          <Icon size={16} />
          <span class="name">{a.filename}</span>
          <span class="size">{formatSize(a.size)}</span>
        </button>
        <button
          class="more"
          title="Más opciones"
          aria-haspopup="menu"
          onclick={(e) => toggleMenu(e, a)}
        >
          <MoreHorizontal size={15} />
        </button>
        {#if menuFor === a.index}
          <div class="menu" role="menu" style:top="{menuPos.top}px" style:left="{menuPos.left}px" style:width="{MENU_W}px">
            {#if a.canOpen}
              <button role="menuitem" onclick={() => run(() => app.openAttachmentDefault(message, a))}><ExternalLink size={15} /> Abrir con la app predeterminada</button>
              <button role="menuitem" onclick={() => run(() => app.openAttachmentWith(message, a))}><AppWindow size={15} /> Abrir con…</button>
            {/if}
            <button role="menuitem" onclick={() => run(() => app.saveAttachment(message, a))}><Download size={15} /> Guardar…</button>
          </div>
        {/if}
      </div>
    {/each}
  </div>
{/if}

<style>
  .chips { display: flex; flex-wrap: wrap; gap: 8px; margin-top: 12px; }
  .chip {
    position: relative; display: flex; align-items: center; max-width: 300px; border-radius: 9px;
    border: 1px solid var(--border); background: var(--surface-2); color: var(--text); font-size: 12.5px;
  }
  .chip:hover { border-color: var(--accent); }
  .main { display: flex; align-items: center; gap: 8px; min-width: 0; padding: 7px 4px 7px 10px; border: none; background: none; color: inherit; font: inherit; text-align: left; }
  .more { display: grid; place-items: center; padding: 6px; margin-right: 3px; border: none; background: none; color: inherit; border-radius: 6px; opacity: 0.55; }
  .more:hover { opacity: 1; background: rgba(127, 127, 127, 0.15); }
  .risky .main > :global(svg:first-child) { color: var(--warning); }
  .compact .chip { background: rgba(127, 127, 127, 0.12); border-color: transparent; color: inherit; }
  .name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-weight: 500; }
  .size { color: var(--text-3); flex-shrink: 0; }
  .compact .size { color: inherit; opacity: 0.7; }
  .menu {
    position: fixed; z-index: 45; padding: 5px;
    background: var(--surface); color: var(--text); border: 1px solid var(--border); border-radius: 10px; box-shadow: var(--shadow-lg);
  }
  .menu button { width: 100%; display: flex; align-items: center; gap: 9px; padding: 7px 10px; border: none; background: none; color: inherit; border-radius: 7px; font-size: 13px; text-align: left; }
  .menu button:hover { background: var(--surface-3); }
</style>
