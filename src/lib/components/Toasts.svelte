<script lang="ts">
  import { app } from '../state.svelte';
  import { CheckCircle2, AlertTriangle, Info } from '@lucide/svelte';
</script>

<div class="toasts" aria-live="polite">
  {#each app.toasts as t (t.id)}
    <div class="toast {t.kind}">
      {#if t.kind === 'success'}<CheckCircle2 size={16} />{:else if t.kind === 'error'}<AlertTriangle size={16} />{:else}<Info size={16} />{/if}
      <span>{t.text}</span>
    </div>
  {/each}
</div>

<style>
  .toasts { position: fixed; left: 50%; bottom: 24px; transform: translateX(-50%); display: flex; flex-direction: column; gap: 8px; z-index: 100; align-items: center; }
  .toast { display: flex; align-items: center; gap: 9px; padding: 10px 16px; border-radius: 10px; background: var(--text); color: var(--surface); box-shadow: var(--shadow-lg); font-size: 13px; max-width: 560px; animation: pop 0.15s ease-out; user-select: text; }
  .toast.success :global(svg) { color: #4ade80; }
  .toast.error { background: var(--danger); color: #fff; }
</style>
