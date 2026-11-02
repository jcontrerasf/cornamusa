<script lang="ts">
  // Renders sanitized email HTML in a sandboxed iframe: no scripts (no allow-scripts), a CSP
  // that blocks every network load except images when the user allowed them, and links
  // intercepted so they open in the system browser (or the composer for mailto:).
  import { onDestroy } from 'svelte';
  import { api } from '../api';
  import { app } from '../state.svelte';
  import { darkTextColor, escapeHtml, isWhiteBackground, looksDarkSafe } from '../util';
  import { linkify } from '../compose';

  type Mode = 'auto' | 'light' | 'dark';
  let {
    html = null,
    text = null,
    allowRemote = false,
    mode = 'auto',
    dark = $bindable(),
  }: { html?: string | null; text?: string | null; allowRemote?: boolean; mode?: Mode; dark?: boolean } = $props();

  let frame: HTMLIFrameElement | undefined = $state();
  let height = $state(80);
  let hoverUrl = $state<string | null>(null);
  let ready = $state(false);
  let observer: ResizeObserver | null = null;

  const DARK_TEXT = '#e8eaf0';

  // Simple mail (plain text, typical replies) follows the dark theme on its own; designed
  // mail (newsletters) stays light unless the user forces it, which inverts the colors.
  const safe = $derived(looksDarkSafe(html));
  const native = $derived(app.isDark && safe && mode !== 'light');
  const inverted = $derived(app.isDark && !safe && mode === 'dark');
  $effect(() => {
    dark = native || inverted;
  });

  const body = $derived(
    html ?? `<div style="white-space:pre-wrap;font-family:inherit">${linkify(escapeHtml(text ?? ''))}</div>`,
  );

  const themeCss = $derived(
    native
      ? `html,body{margin:0;padding:0;background:transparent;color:${DARK_TEXT};color-scheme:dark}
  blockquote{margin:0 0 0 .8ex;border-left:2px solid #3a3f4c;padding-left:1ex;color:#a5abb9}
  a{color:#9494ff}`
      : `html,body{margin:0;padding:0;background:#fff;color:#1b1f2a}
  blockquote{margin:0 0 0 .8ex;border-left:2px solid #ccd;padding-left:1ex;color:#555}
  a{color:#4646d8}` +
          (inverted
            ? `
  html{filter:invert(1) hue-rotate(180deg)}
  img,video,picture,svg,[background],[style*="background-image"],[style*="background:url"]{filter:invert(1) hue-rotate(180deg)}`
            : ''),
  );

  const srcdoc = $derived.by(() => {
    const remote = allowRemote ? ' https: http:' : '';
    return `<!doctype html><html><head><meta charset="utf-8">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; img-src data:${remote}; style-src 'unsafe-inline'${remote}; font-src data:${remote}; media-src 'none'; frame-src 'none'; form-action 'none'">
<style>
  ${themeCss}
  body{font-family:system-ui,-apple-system,'Segoe UI',sans-serif;font-size:14px;line-height:1.5;padding:4px 2px;overflow-wrap:anywhere}
  img{max-width:100%;height:auto}
  table{max-width:100%}
  pre{white-space:pre-wrap}
</style></head><body>${body}</body></html>`;
  });

  // Hide the frame while a new document loads so dark mode doesn't flash unfixed colors.
  $effect.pre(() => {
    void srcdoc;
    ready = false;
  });

  /** Makes leftover explicit colors readable on the dark background. */
  function fixDarkColors(doc: Document) {
    const win = doc.defaultView;
    if (!win) return;
    for (const el of doc.body.querySelectorAll<HTMLElement>('*')) {
      const cs = win.getComputedStyle(el);
      const fg = darkTextColor(cs.color, DARK_TEXT);
      if (fg) el.style.setProperty('color', fg, 'important');
      if (isWhiteBackground(cs.backgroundColor)) el.style.setProperty('background-color', 'transparent', 'important');
    }
  }

  function measure() {
    const doc = frame?.contentDocument;
    if (!doc?.documentElement) return;
    height = Math.max(40, doc.documentElement.scrollHeight);
  }

  function onLoad() {
    const doc = frame?.contentDocument;
    if (!doc) return;
    if (native && doc.body) fixDarkColors(doc);
    ready = true;
    measure();
    observer?.disconnect();
    observer = new ResizeObserver(measure);
    observer.observe(doc.body);
    doc.querySelectorAll('img').forEach((img) => img.addEventListener('load', measure));

    doc.addEventListener('click', (e) => {
      const a = (e.target as Element | null)?.closest?.('a');
      if (!a) return;
      e.preventDefault();
      const href = a.getAttribute('href') ?? '';
      if (href.toLowerCase().startsWith('mailto:')) {
        const addr = decodeURIComponent(href.slice(7).split('?')[0]);
        app.compose({ mode: 'new', to: [{ name: null, email: addr }] });
      } else if (/^https?:/i.test(href)) {
        api.openExternal(href).catch((err) => app.error(err));
      }
    });
    // Show the real destination of a link (anti-phishing).
    doc.addEventListener('mouseover', (e) => {
      const a = (e.target as Element | null)?.closest?.('a');
      hoverUrl = a?.getAttribute('href') ?? null;
    });
    doc.addEventListener('mouseleave', () => (hoverUrl = null));
  }

  onDestroy(() => observer?.disconnect());
</script>

<div class="wrap">
  <iframe
    bind:this={frame}
    title="Contenido del mensaje"
    sandbox="allow-same-origin"
    referrerpolicy="no-referrer"
    {srcdoc}
    class:dark={native}
    style:height="{height}px"
    style:visibility={ready ? 'visible' : 'hidden'}
    onload={onLoad}
  ></iframe>
  {#if hoverUrl}
    <div class="status">{hoverUrl}</div>
  {/if}
</div>

<style>
  .wrap { position: relative; }
  iframe { width: 100%; border: none; display: block; background: #fff; border-radius: 8px; color-scheme: light; }
  iframe.dark { background: transparent; color-scheme: dark; }
  .status {
    position: fixed; left: 50%; bottom: 14px; transform: translateX(-50%); max-width: 60vw;
    background: var(--text); color: var(--surface); padding: 5px 10px; border-radius: 6px; font-size: 12px;
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap; z-index: 40; pointer-events: none; box-shadow: var(--shadow);
  }
</style>
