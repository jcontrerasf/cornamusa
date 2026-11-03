<script lang="ts">
  // Built-in viewer for image and PDF attachments. Bytes come from the backend as an
  // ArrayBuffer; images are shown from a blob: URL and PDFs are rendered with pdf.js
  // (loaded on demand, eval disabled, no PDF scripting).
  import { onDestroy, tick, untrack } from 'svelte';
  import { api, type AttachmentInfo } from '../api';
  import { app } from '../state.svelte';
  import { formatSize } from '../util';
  import { X, ChevronLeft, ChevronRight, ZoomIn, ZoomOut, Maximize, ExternalLink, AppWindow, Download } from '@lucide/svelte';
  import type { PDFDocumentProxy } from 'pdfjs-dist';

  const message = $derived(app.viewer!.message);
  const items = $derived(message.attachments.filter((a) => !a.inline && (a.kind === 'image' || a.kind === 'pdf')));
  const pos = $derived(Math.max(0, items.findIndex((a) => a.index === app.viewer!.index)));
  const current = $derived<AttachmentInfo | undefined>(items[pos]);

  let loading = $state(true);
  let failed = $state<string | null>(null);
  let imageUrl = $state<string | null>(null);
  let actualSize = $state(false);

  // PDF state
  let pdf = $state.raw<PDFDocumentProxy | null>(null);
  let pageCount = $state(0);
  let base = $state({ width: 612, height: 792 }); // page 1 size at scale 1 (points)
  let scale = $state(1);
  let fitWidth = $state(true);
  let currentPage = $state(1);
  let scroller: HTMLElement | undefined = $state();
  let loadToken = 0;

  function cleanup() {
    if (imageUrl) URL.revokeObjectURL(imageUrl);
    imageUrl = null;
    pdf?.destroy();
    pdf = null;
    pageCount = 0;
  }

  async function load(a: AttachmentInfo) {
    const token = ++loadToken;
    cleanup();
    loading = true;
    failed = null;
    actualSize = false;
    try {
      const bytes = await api.readAttachment(message.summary.id, a.index);
      if (token !== loadToken) return;
      if (a.kind === 'image') {
        imageUrl = URL.createObjectURL(new Blob([bytes], { type: a.mimeType.startsWith('image/') ? a.mimeType : '' }));
      } else {
        const pdfjs = await import('pdfjs-dist');
        const worker = await import('pdfjs-dist/build/pdf.worker.min.mjs?url');
        pdfjs.GlobalWorkerOptions.workerSrc = worker.default;
        const doc = await pdfjs.getDocument({
          data: new Uint8Array(bytes),
          isEvalSupported: false,
          cMapUrl: '/pdfjs/cmaps/',
          cMapPacked: true,
          standardFontDataUrl: '/pdfjs/standard_fonts/',
        }).promise;
        if (token !== loadToken) {
          doc.destroy();
          return;
        }
        const first = (await doc.getPage(1)).getViewport({ scale: 1 });
        base = { width: first.width, height: first.height };
        pdf = doc;
        pageCount = doc.numPages;
        currentPage = 1;
        fitWidth = true;
        await tick();
        applyFit();
      }
    } catch (e) {
      if (token === loadToken) failed = String(e);
    } finally {
      if (token === loadToken) loading = false;
    }
  }

  $effect(() => {
    const a = current;
    // load() reads and resets viewer state; only `current` should re-trigger it.
    if (a) untrack(() => load(a));
  });
  onDestroy(() => {
    loadToken++;
    cleanup();
  });

  function applyFit() {
    if (!scroller) return;
    const avail = Math.min(scroller.clientWidth - 48, 960);
    scale = Math.max(0.3, avail / base.width);
  }

  function zoom(factor: number) {
    fitWidth = false;
    scale = Math.min(5, Math.max(0.25, scale * factor));
  }

  function go(delta: number) {
    const next = items[pos + delta];
    if (next) app.viewer = { message, index: next.index };
  }

  function close() {
    app.viewer = null;
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Escape') close();
    else if (e.key === 'ArrowLeft' && items.length > 1) go(-1);
    else if (e.key === 'ArrowRight' && items.length > 1) go(1);
    else if (e.key === '+' || e.key === '=') zoom(1.2);
    else if (e.key === '-') zoom(1 / 1.2);
    else return;
    e.preventDefault();
  }

  // ---------- PDF pages: rendered when they scroll into view ----------

  /** Svelte action: renders page `n` into the node when it becomes visible. */
  function pdfPage(node: HTMLElement, params: { n: number; scale: number }) {
    let rendered = false;
    let task: { cancel(): void } | null = null;

    async function render() {
      if (rendered || !pdf) return;
      rendered = true;
      const doc = pdf;
      const page = await doc.getPage(params.n);
      const viewport = page.getViewport({ scale: params.scale });
      const ratio = window.devicePixelRatio || 1;
      const canvas = document.createElement('canvas');
      canvas.width = Math.floor(viewport.width * ratio);
      canvas.height = Math.floor(viewport.height * ratio);
      canvas.style.width = `${viewport.width}px`;
      canvas.style.height = `${viewport.height}px`;
      node.style.width = `${viewport.width}px`;
      node.style.height = `${viewport.height}px`;
      const ctx = canvas.getContext('2d')!;
      const renderTask = page.render({
        canvasContext: ctx,
        viewport,
        transform: ratio !== 1 ? [ratio, 0, 0, ratio, 0, 0] : undefined,
      });
      task = renderTask;
      try {
        await renderTask.promise;
      } catch {
        return; // cancelled
      }
      node.replaceChildren(canvas);
      const pdfjs = await import('pdfjs-dist');
      const textDiv = document.createElement('div');
      textDiv.className = 'textLayer';
      node.append(textDiv);
      await new pdfjs.TextLayer({ textContentSource: page.streamTextContent(), container: textDiv, viewport }).render();
    }

    const io = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          if (entry.isIntersecting) render();
        }
      },
      { root: scroller, rootMargin: '400px 0px' },
    );
    io.observe(node);
    return {
      destroy() {
        io.disconnect();
        task?.cancel();
      },
    };
  }

  /** Page indicator: the page under the middle of the visible area. */
  function onScroll() {
    if (!pdf || !scroller) return;
    const mid = scroller.getBoundingClientRect().top + scroller.clientHeight / 2;
    const pages = scroller.querySelectorAll<HTMLElement>('.page');
    for (let i = 0; i < pages.length; i++) {
      const r = pages[i].getBoundingClientRect();
      if (r.bottom >= mid) {
        currentPage = i + 1;
        return;
      }
    }
  }

  // ---------- image panning when shown at actual size ----------
  let drag: { x: number; y: number; left: number; top: number } | null = null;
  let moved = false;
  function onDown(e: PointerEvent) {
    if (!actualSize || !scroller) return;
    drag = { x: e.clientX, y: e.clientY, left: scroller.scrollLeft, top: scroller.scrollTop };
    moved = false;
  }
  function onMove(e: PointerEvent) {
    if (!drag || !scroller) return;
    const dx = e.clientX - drag.x;
    const dy = e.clientY - drag.y;
    if (Math.abs(dx) + Math.abs(dy) > 3) moved = true;
    scroller.scrollLeft = drag.left - dx;
    scroller.scrollTop = drag.top - dy;
  }
  function onUp() {
    drag = null;
  }
  function toggleSize() {
    if (!moved) actualSize = !actualSize;
    moved = false;
  }
</script>

<svelte:window onkeydown={onKey} onpointermove={onMove} onpointerup={onUp} onresize={() => fitWidth && applyFit()} />

<div class="viewer" role="dialog" aria-label="Visor de adjuntos">
  <header>
    <div class="title">
      <strong title={current?.filename}>{current?.filename}</strong>
      <span class="faint">
        {current ? formatSize(current.size) : ''}
        {#if pdf} · página {currentPage} de {pageCount}{/if}
        {#if items.length > 1} · adjunto {pos + 1} de {items.length}{/if}
      </span>
    </div>
    {#if items.length > 1}
      <button class="icon-btn" title="Anterior (←)" onclick={() => go(-1)} disabled={pos === 0}><ChevronLeft size={18} /></button>
      <button class="icon-btn" title="Siguiente (→)" onclick={() => go(1)} disabled={pos === items.length - 1}><ChevronRight size={18} /></button>
      <span class="sep"></span>
    {/if}
    {#if pdf}
      <button class="icon-btn" title="Alejar (−)" onclick={() => zoom(1 / 1.2)}><ZoomOut size={18} /></button>
      <span class="zoom">{Math.round(scale * 100)}%</span>
      <button class="icon-btn" title="Acercar (+)" onclick={() => zoom(1.2)}><ZoomIn size={18} /></button>
      <button class="icon-btn" class:active={fitWidth} title="Ajustar al ancho" onclick={() => { fitWidth = true; applyFit(); }}><Maximize size={17} /></button>
      <span class="sep"></span>
    {/if}
    {#if current?.canOpen}
      <button class="icon-btn" title="Abrir con la aplicación predeterminada" onclick={() => current && app.openAttachmentDefault(message, current)}><ExternalLink size={17} /></button>
      <button class="icon-btn" title="Abrir con…" onclick={() => current && app.openAttachmentWith(message, current)}><AppWindow size={17} /></button>
    {/if}
    <button class="icon-btn" title="Guardar" onclick={() => current && app.saveAttachment(message, current)}><Download size={17} /></button>
    <button class="icon-btn" title="Cerrar (Esc)" onclick={close}><X size={19} /></button>
  </header>

  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="stage" class:pdf={!!pdf} class:actual={actualSize} bind:this={scroller} onpointerdown={onDown} onscroll={onScroll}>
    {#if loading}
      <p class="msg">Cargando…</p>
    {:else if failed}
      <p class="msg">No se pudo mostrar el adjunto: {failed}</p>
    {:else if imageUrl}
      <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
      <img src={imageUrl} alt={current?.filename} onclick={toggleSize} draggable="false" title={actualSize ? 'Ajustar a la ventana' : 'Ver a tamaño real'} />
    {:else if pdf}
      {#key scale}
        <div class="pages">
          {#each Array.from({ length: pageCount }, (_, i) => i + 1) as n (n)}
            <div class="page" style:width="{base.width * scale}px" style:height="{base.height * scale}px" style:--scale-factor={scale} use:pdfPage={{ n, scale }}></div>
          {/each}
        </div>
      {/key}
    {/if}
  </div>
</div>

<style>
  .viewer { position: fixed; inset: 0; z-index: 60; display: flex; flex-direction: column; background: rgba(12, 14, 20, 0.97); color: #e8eaf0; }
  header { display: flex; align-items: center; gap: 4px; padding: 8px 12px; background: rgba(0, 0, 0, 0.35); border-bottom: 1px solid rgba(255, 255, 255, 0.08); }
  header .icon-btn { color: #e8eaf0; }
  header .icon-btn:hover { background: rgba(255, 255, 255, 0.1); }
  header .icon-btn.active { color: #9494ff; }
  .title { flex: 1; min-width: 0; display: flex; flex-direction: column; padding: 0 6px; }
  .title strong { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 14px; }
  .title .faint { font-size: 12px; color: #a5abb9; }
  .sep { width: 1px; height: 20px; background: rgba(255, 255, 255, 0.15); margin: 0 6px; }
  .zoom { font-size: 12px; min-width: 42px; text-align: center; color: #a5abb9; }
  .stage { flex: 1; overflow: auto; display: grid; place-items: center; padding: 24px; min-height: 0; }
  .stage.pdf { place-items: start center; }
  .stage img { max-width: 100%; max-height: calc(100vh - 110px); object-fit: contain; cursor: zoom-in; user-select: none; background: repeating-conic-gradient(#2a2e38 0% 25%, #1f232c 0% 50%) 50% / 20px 20px; }
  .stage.actual { place-items: start; }
  .stage.actual img { max-width: none; max-height: none; cursor: grab; }
  .msg { color: #a5abb9; }
  .pages { display: flex; flex-direction: column; align-items: center; gap: 14px; }
  .page { position: relative; background: #fff; box-shadow: 0 4px 24px rgba(0, 0, 0, 0.5); }
  .page :global(canvas) { display: block; }
  /* pdf.js text layer (subset of pdf_viewer.css): transparent text over the canvas, for selection. */
  .page :global(.textLayer) { position: absolute; inset: 0; overflow: clip; opacity: 1; line-height: 1; text-align: initial; transform-origin: 0 0; z-index: 0; -webkit-text-size-adjust: none; text-size-adjust: none; }
  .page :global(.textLayer span), .page :global(.textLayer br) { color: transparent; position: absolute; white-space: pre; cursor: text; transform-origin: 0% 0%; }
  .page :global(.textLayer ::selection) { background: rgba(0, 100, 255, 0.3); }
  .page :global(.textLayer .endOfContent) { display: block; position: absolute; inset: 100% 0 0; z-index: -1; cursor: default; user-select: none; }
</style>
