<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import { Editor } from '@tiptap/core';
  import { StarterKit } from '@tiptap/starter-kit';
  import { TextAlign } from '@tiptap/extension-text-align';
  import { TextStyle, Color } from '@tiptap/extension-text-style';
  import { Image } from '@tiptap/extension-image';
  import { Placeholder } from '@tiptap/extensions';
  import {
    Bold, Italic, Underline as UnderlineIcon, Strikethrough, List, ListOrdered, Quote, Link2,
    AlignLeft, AlignCenter, AlignRight, Undo2, Redo2, RemoveFormatting, Palette, Image as ImageIcon,
  } from '@lucide/svelte';

  let {
    content = '',
    placeholder = 'Escribe tu mensaje…',
    minHeight = 220,
    onchange,
  }: { content?: string; placeholder?: string; minHeight?: number; onchange?: (html: string, text: string) => void } = $props();

  let el: HTMLDivElement | undefined = $state();
  let editor = $state<Editor | null>(null);
  let tick = $state(0); // re-render toolbar state on transactions

  onMount(() => {
    editor = new Editor({
      element: el!,
      extensions: [
        // StarterKit (v3) already bundles Underline and Link.
        StarterKit.configure({
          link: { openOnClick: false, autolink: true, HTMLAttributes: { rel: 'noopener noreferrer' } },
        }),
        TextAlign.configure({ types: ['heading', 'paragraph'] }),
        TextStyle,
        Color,
        Image.configure({ inline: true }),
        Placeholder.configure({ placeholder }),
      ],
      content,
      onTransaction: () => (tick += 1),
      onUpdate: ({ editor }) => onchange?.(editor.getHTML(), editor.getText({ blockSeparator: '\n' })),
    });
  });

  onDestroy(() => editor?.destroy());

  export function focus() {
    editor?.commands.focus('start');
  }
  export function setContent(html: string) {
    editor?.commands.setContent(html, { emitUpdate: true });
  }

  const is = (name: string, attrs?: Record<string, unknown>) => {
    void tick;
    return editor?.isActive(name, attrs) ?? false;
  };
  const isAlign = (a: string) => {
    void tick;
    return editor?.isActive({ textAlign: a }) ?? false;
  };

  function setLink() {
    if (!editor) return;
    const prev = editor.getAttributes('link').href as string | undefined;
    const url = window.prompt('Dirección del enlace', prev ?? 'https://');
    if (url === null) return;
    if (url === '' || url === 'https://') editor.chain().focus().unsetLink().run();
    else if (/^(https?:|mailto:)/i.test(url)) editor.chain().focus().extendMarkRange('link').setLink({ href: url }).run();
  }

  function addImage() {
    const url = window.prompt('URL de la imagen (https://…)');
    if (url && /^https:\/\//i.test(url)) editor?.chain().focus().setImage({ src: url }).run();
  }

  const COLORS = ['#1b1f2a', '#e5484d', '#f59e0b', '#16a34a', '#0ea5e9', '#5b5bf0', '#ec4899', '#8b92a3'];
  let showColors = $state(false);
</script>

<div class="editor">
  <div class="toolbar">
    <button type="button" class="icon-btn" class:active={is('bold')} title="Negrita (Ctrl+B)" onclick={() => editor?.chain().focus().toggleBold().run()}><Bold size={16} /></button>
    <button type="button" class="icon-btn" class:active={is('italic')} title="Cursiva (Ctrl+I)" onclick={() => editor?.chain().focus().toggleItalic().run()}><Italic size={16} /></button>
    <button type="button" class="icon-btn" class:active={is('underline')} title="Subrayado (Ctrl+U)" onclick={() => editor?.chain().focus().toggleUnderline().run()}><UnderlineIcon size={16} /></button>
    <button type="button" class="icon-btn" class:active={is('strike')} title="Tachado" onclick={() => editor?.chain().focus().toggleStrike().run()}><Strikethrough size={16} /></button>
    <div class="color">
      <button type="button" class="icon-btn" title="Color" onclick={() => (showColors = !showColors)}><Palette size={16} /></button>
      {#if showColors}
        <div class="swatches">
          {#each COLORS as c}
            <button type="button" aria-label={c} style:background={c} onclick={() => { editor?.chain().focus().setColor(c).run(); showColors = false; }}></button>
          {/each}
        </div>
      {/if}
    </div>
    <span class="sep"></span>
    <button type="button" class="icon-btn" class:active={is('bulletList')} title="Lista" onclick={() => editor?.chain().focus().toggleBulletList().run()}><List size={16} /></button>
    <button type="button" class="icon-btn" class:active={is('orderedList')} title="Lista numerada" onclick={() => editor?.chain().focus().toggleOrderedList().run()}><ListOrdered size={16} /></button>
    <button type="button" class="icon-btn" class:active={is('blockquote')} title="Cita" onclick={() => editor?.chain().focus().toggleBlockquote().run()}><Quote size={16} /></button>
    <span class="sep"></span>
    <button type="button" class="icon-btn" class:active={isAlign('left')} title="Alinear a la izquierda" onclick={() => editor?.chain().focus().setTextAlign('left').run()}><AlignLeft size={16} /></button>
    <button type="button" class="icon-btn" class:active={isAlign('center')} title="Centrar" onclick={() => editor?.chain().focus().setTextAlign('center').run()}><AlignCenter size={16} /></button>
    <button type="button" class="icon-btn" class:active={isAlign('right')} title="Alinear a la derecha" onclick={() => editor?.chain().focus().setTextAlign('right').run()}><AlignRight size={16} /></button>
    <span class="sep"></span>
    <button type="button" class="icon-btn" class:active={is('link')} title="Enlace" onclick={setLink}><Link2 size={16} /></button>
    <button type="button" class="icon-btn" title="Imagen desde URL" onclick={addImage}><ImageIcon size={16} /></button>
    <button type="button" class="icon-btn" title="Quitar formato" onclick={() => editor?.chain().focus().unsetAllMarks().clearNodes().run()}><RemoveFormatting size={16} /></button>
    <span class="grow"></span>
    <button type="button" class="icon-btn" title="Deshacer" onclick={() => editor?.chain().focus().undo().run()}><Undo2 size={16} /></button>
    <button type="button" class="icon-btn" title="Rehacer" onclick={() => editor?.chain().focus().redo().run()}><Redo2 size={16} /></button>
  </div>
  <div class="content selectable" bind:this={el} style:min-height="{minHeight}px"></div>
</div>

<style>
  .editor { display: flex; flex-direction: column; min-height: 0; }
  .toolbar { display: flex; align-items: center; flex-wrap: wrap; gap: 1px; padding: 4px 6px; border-bottom: 1px solid var(--border); }
  .toolbar .icon-btn { width: 30px; height: 30px; }
  .sep { width: 1px; height: 18px; background: var(--border); margin: 0 4px; }
  .grow { flex: 1; }
  .color { position: relative; }
  .swatches { position: absolute; top: 34px; left: 0; display: grid; grid-template-columns: repeat(4, 22px); gap: 6px; padding: 8px; background: var(--surface); border: 1px solid var(--border); border-radius: 10px; box-shadow: var(--shadow); z-index: 5; }
  .swatches button { width: 22px; height: 22px; border-radius: 50%; border: 2px solid var(--surface); box-shadow: 0 0 0 1px var(--border); }
  .content { flex: 1; overflow-y: auto; padding: 14px 18px; cursor: text; }
  .content :global(.ProseMirror) { outline: none; min-height: inherit; line-height: 1.55; }
  .content :global(.ProseMirror p) { margin: 0 0 0.5em; }
  .content :global(.ProseMirror blockquote) { border-left: 3px solid var(--border); margin: 0.5em 0; padding-left: 12px; color: var(--text-2); }
  .content :global(.ProseMirror a) { color: var(--accent); }
  .content :global(.ProseMirror img) { max-width: 100%; }
  .content :global(.ProseMirror p.is-editor-empty:first-child::before) { content: attr(data-placeholder); color: var(--text-3); float: left; height: 0; pointer-events: none; }
</style>
