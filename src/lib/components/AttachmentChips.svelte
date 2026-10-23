<script lang="ts">
  import { api, type MessageBody } from '../api';
  import { app } from '../state.svelte';
  import { formatSize } from '../util';
  import { Paperclip, Download, FileImage, FileText, FileArchive } from '@lucide/svelte';

  let { message, compact = false }: { message: MessageBody; compact?: boolean } = $props();
  const files = $derived(message.attachments.filter((a) => !a.inline));

  function icon(mime: string) {
    if (mime.startsWith('image/')) return FileImage;
    if (mime.includes('zip') || mime.includes('compressed') || mime.includes('tar')) return FileArchive;
    if (mime.includes('pdf') || mime.startsWith('text/') || mime.includes('document')) return FileText;
    return Paperclip;
  }

  async function save(index: number) {
    try {
      if (await api.saveAttachment(message.summary.id, index)) app.toast('Adjunto guardado', 'success');
    } catch (e) {
      app.error(e);
    }
  }
</script>

{#if files.length}
  <div class="chips" class:compact>
    {#each files as a (a.index)}
      {@const Icon = icon(a.mimeType)}
      <button class="chip" onclick={() => save(a.index)} title="Guardar {a.filename}">
        <Icon size={16} />
        <span class="name">{a.filename}</span>
        <span class="size">{formatSize(a.size)}</span>
        <Download size={14} class="dl" />
      </button>
    {/each}
  </div>
{/if}

<style>
  .chips { display: flex; flex-wrap: wrap; gap: 8px; margin-top: 12px; }
  .chip {
    display: flex; align-items: center; gap: 8px; padding: 7px 10px; border-radius: 9px; max-width: 280px;
    border: 1px solid var(--border); background: var(--surface-2); color: var(--text); font-size: 12.5px;
  }
  .chip:hover { border-color: var(--accent); }
  .compact .chip { background: rgba(127, 127, 127, 0.12); border-color: transparent; color: inherit; }
  .name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-weight: 500; }
  .size { color: var(--text-3); flex-shrink: 0; }
  .compact .size { color: inherit; opacity: 0.7; }
  .chip :global(.dl) { opacity: 0.6; flex-shrink: 0; }
</style>
