<script lang="ts">
  import { app } from '../state.svelte';
  import { api, type Account, type SendKey } from '../api';
  import SignatureEditor from './SignatureEditor.svelte';
  import { X, SlidersHorizontal, Users, PenLine, Trash2, Plus, RefreshCw, Play, FolderOpen, Volume2 } from '@lucide/svelte';
  import { SOUNDS, CUSTOM_SOUND, isBundledSound, playSound, invalidateCustomSound, soundFor } from '../sounds';
  import { FOLDER_ICONS } from '../folders';

  const COLORS = ['#6366f1', '#ec4899', '#14b8a6', '#f59e0b', '#8b5cf6', '#ef4444', '#22c55e', '#0ea5e9'];
  const PROVIDER: Record<string, string> = { gmail: 'Gmail', outlook: 'Outlook / Microsoft 365', imap: 'IMAP' };

  const tab = $derived(app.showSettings ?? 'general');
  const s = $derived(app.settings);

  async function toggle(key: 'closeToTray' | 'notifications' | 'doNotDisturb' | 'loadRemoteImages' | 'startMinimized' | 'autostart') {
    if (!app.settings) return;
    app.settings[key] = !app.settings[key];
    await app.saveSettings();
  }

  async function setSound(id: string) {
    if (!app.settings) return;
    app.settings.newMailSound = id;
    playSound(id);
    await app.saveSettings();
  }

  async function pickCustomSound() {
    if (!app.settings) return;
    try {
      const name = await api.pickCustomSound();
      if (!name) return;
      invalidateCustomSound();
      app.settings.customSoundName = name;
      app.settings.newMailSound = CUSTOM_SOUND;
      await app.saveSettings();
      if (!(await playSound(CUSTOM_SOUND))) app.toast('No se pudo reproducir ese archivo; prueba con otro formato (WAV, MP3 u OGG)', 'error');
    } catch (e) {
      app.error(e);
    }
  }

  async function setSendKey(key: 'chatSendKey' | 'composerSendKey', value: SendKey) {
    if (!app.settings) return;
    app.settings[key] = value;
    await app.saveSettings();
  }

  const soundLabel = (id: string) =>
    id === CUSTOM_SOUND ? `Propio${s?.customSoundName ? `: ${s.customSoundName}` : ''}`
    : id === 'system' ? 'Sonido del sistema'
    : id === 'none' ? 'Sin sonido'
    : (SOUNDS.find((o) => o.id === id)?.label ?? id);

  async function setAccountSound(accountId: string, id: string) {
    if (!app.settings) return;
    const sounds = { ...(app.settings.accountSounds ?? {}) };
    if (id === '') delete sounds[accountId];
    else sounds[accountId] = id;
    app.settings.accountSounds = sounds;
    playSound(soundFor(app.settings, accountId));
    await app.saveSettings();
  }

  async function setTheme(theme: 'system' | 'light' | 'dark') {
    if (!app.settings) return;
    app.settings.theme = theme;
    await app.saveSettings();
  }

  async function saveAccount(a: Account) {
    try {
      await api.updateAccount(a.id, a.displayName, a.color);
    } catch (e) {
      app.error(e);
    }
  }

  async function remove(a: Account) {
    if (!confirm(`¿Quitar la cuenta ${a.email}? Se borrará la caché local y las credenciales guardadas en el llavero del sistema. Tu correo en el servidor no se toca.`)) return;
    try {
      await api.removeAccount(a.id);
      app.toast('Cuenta eliminada', 'success');
    } catch (e) {
      app.error(e);
    }
  }
</script>

<div class="overlay" role="presentation" onclick={(e) => e.target === e.currentTarget && (app.showSettings = null)}>
  <div class="modal settings" class:wide={tab === 'signatures'} role="dialog" aria-label="Configuración">
    <nav>
      <h3>Configuración</h3>
      <button class:active={tab === 'general'} onclick={() => (app.showSettings = 'general')}><SlidersHorizontal size={16} /> General</button>
      <button class:active={tab === 'accounts'} onclick={() => (app.showSettings = 'accounts')}><Users size={16} /> Cuentas</button>
      <button class:active={tab === 'signatures'} onclick={() => (app.showSettings = 'signatures')}><PenLine size={16} /> Firmas</button>
    </nav>
    <section>
      <button class="icon-btn close" onclick={() => (app.showSettings = null)} aria-label="Cerrar"><X size={18} /></button>

      {#if tab === 'general' && s}
        <h2>General</h2>
        <div class="group">
          <div class="opt">
            <div><strong>Al cerrar, minimizar a la bandeja del sistema</strong><p>Cornamusa sigue recibiendo correo y notificando. Para salir usa «Salir» en el ícono de la bandeja.</p></div>
            <label class="switch"><input type="checkbox" checked={s.closeToTray} onchange={() => toggle('closeToTray')} /><span></span></label>
          </div>
          <div class="opt">
            <div><strong>Iniciar con el sistema</strong><p>Abre Cornamusa minimizado en la bandeja al iniciar sesión.</p></div>
            <label class="switch"><input type="checkbox" checked={s.autostart} onchange={() => toggle('autostart')} /><span></span></label>
          </div>
          <div class="opt">
            <div><strong>Iniciar minimizado</strong><p>No mostrar la ventana al abrir la aplicación.</p></div>
            <label class="switch"><input type="checkbox" checked={s.startMinimized} onchange={() => toggle('startMinimized')} /><span></span></label>
          </div>
        </div>
        <div class="group">
          <div class="opt">
            <div><strong>Notificaciones de correo nuevo</strong></div>
            <label class="switch"><input type="checkbox" checked={s.notifications} onchange={() => toggle('notifications')} /><span></span></label>
          </div>
          <div class="opt sound">
            <div><strong>Sonido de correo nuevo</strong><p>Suena junto con la notificación (no con «No molestar»). Cada cuenta puede tener el suyo o ninguno, en «Cuentas».</p></div>
            <div class="sounds" role="radiogroup" aria-label="Sonido de correo nuevo">
              {#each [...SOUNDS, { id: 'system', label: 'Sonido del sistema' }, { id: 'none', label: 'Sin sonido' }] as o (o.id)}
                <div class="sound-row">
                  <label>
                    <input type="radio" name="sound" value={o.id} checked={s.newMailSound === o.id} disabled={!s.notifications} onchange={() => setSound(o.id)} />
                    {o.label}
                  </label>
                  {#if isBundledSound(o.id)}
                    <button class="icon-btn" title="Probar" onclick={() => playSound(o.id)}><Play size={14} /></button>
                  {/if}
                </div>
              {/each}
              <div class="sound-row">
                <label title={s.customSoundName ?? undefined}>
                  <input type="radio" name="sound" value={CUSTOM_SOUND} checked={s.newMailSound === CUSTOM_SOUND} disabled={!s.notifications || !s.customSoundName} onchange={() => setSound(CUSTOM_SOUND)} />
                  <span class="custom-name">{s.customSoundName ? `Propio: ${s.customSoundName}` : 'Sonido propio'}</span>
                </label>
                <span class="custom-actions">
                  {#if s.customSoundName}
                    <button class="icon-btn" title="Probar" onclick={() => playSound(CUSTOM_SOUND)}><Play size={14} /></button>
                  {/if}
                  <button class="icon-btn" title="Elegir archivo de audio…" disabled={!s.notifications} onclick={pickCustomSound}><FolderOpen size={14} /></button>
                </span>
              </div>
            </div>
          </div>
          <div class="opt">
            <div><strong>No molestar</strong><p>Silencia temporalmente las notificaciones (también desde la bandeja).</p></div>
            <label class="switch"><input type="checkbox" checked={s.doNotDisturb} onchange={() => toggle('doNotDisturb')} /><span></span></label>
          </div>
        </div>
        <div class="group">
          <div class="opt">
            <div><strong>Enviar desde la vista chat</strong><p>Con Ctrl+Enter o desactivado, Enter agrega una línea y no envía por accidente.</p></div>
            <div class="seg">
              {#each [['enter', 'Enter'], ['ctrlEnter', 'Ctrl+Enter'], ['none', 'Solo el botón']] as [v, l]}
                <button class:on={s.chatSendKey === v} onclick={() => setSendKey('chatSendKey', v as SendKey)}>{l}</button>
              {/each}
            </div>
          </div>
          <div class="opt">
            <div><strong>Enviar desde el editor completo</strong></div>
            <div class="seg">
              {#each [['ctrlEnter', 'Ctrl+Enter'], ['none', 'Solo el botón']] as [v, l]}
                <button class:on={s.composerSendKey === v} onclick={() => setSendKey('composerSendKey', v as SendKey)}>{l}</button>
              {/each}
            </div>
          </div>
        </div>
        <div class="group">
          <div class="opt">
            <div><strong>Cargar imágenes remotas siempre</strong><p>Desactivado por privacidad: las imágenes remotas permiten a los remitentes saber cuándo y dónde abres un correo.</p></div>
            <label class="switch"><input type="checkbox" checked={s.loadRemoteImages} onchange={() => toggle('loadRemoteImages')} /><span></span></label>
          </div>
          <div class="opt">
            <div><strong>Tema</strong></div>
            <div class="seg">
              {#each [['system', 'Sistema'], ['light', 'Claro'], ['dark', 'Oscuro']] as [v, l]}
                <button class:on={s.theme === v} onclick={() => setTheme(v as 'system' | 'light' | 'dark')}>{l}</button>
              {/each}
            </div>
          </div>
        </div>
      {:else if tab === 'accounts'}
        <h2>Cuentas</h2>
        {#each app.accounts as a (a.id)}
          <div class="account">
            <div class="colors">
              {#each COLORS as c}
                <button aria-label="Color {c}" class:sel={a.color === c} style:background={c} onclick={() => { a.color = c; saveAccount(a); }}></button>
              {/each}
            </div>
            <div class="info">
              <input class="input" bind:value={a.displayName} onchange={() => saveAccount(a)} title="Nombre que verán los destinatarios" />
              <div class="faint small">{a.email} · {a.authUser ? `Buzón compartido · vía ${a.authUser}` : `${PROVIDER[a.provider]} · ${a.imap.host}`}</div>
              {#if app.syncErrors[a.id]}<div class="error small">{app.syncErrors[a.id]}</div>{/if}
            </div>
            {#if s}
              <label class="acct-sound" title={s.notifications ? 'Sonido de correo nuevo de esta cuenta' : 'Las notificaciones están desactivadas'}>
                <Volume2 size={15} />
                <select class="input" value={s.accountSounds?.[a.id] ?? ''} disabled={!s.notifications} onchange={(e) => setAccountSound(a.id, e.currentTarget.value)} aria-label="Sonido de correo nuevo · {a.email}">
                  <option value="">General ({soundLabel(s.newMailSound)})</option>
                  {#each SOUNDS as o (o.id)}<option value={o.id}>{o.label}</option>{/each}
                  {#if s.customSoundName}<option value={CUSTOM_SOUND}>{soundLabel(CUSTOM_SOUND)}</option>{/if}
                  <option value="system">Sonido del sistema</option>
                  <option value="none">Sin sonido</option>
                </select>
              </label>
            {/if}
            <button class="icon-btn" title="Sincronizar ahora" onclick={() => api.syncAccount(a.id).catch((e) => app.error(e))}><RefreshCw size={16} /></button>
            <button class="icon-btn" title="Quitar cuenta" onclick={() => remove(a)}><Trash2 size={16} /></button>
          </div>
          {#if (app.folders[a.id] ?? []).length > 0}
            <details class="folders">
              <summary>Carpetas en la barra lateral{(app.folders[a.id] ?? []).some((f) => app.isHidden(f)) ? ` · ${(app.folders[a.id] ?? []).filter((f) => app.isHidden(f)).length} ocultas` : ''}</summary>
              <p class="faint small">Las carpetas ocultas no aparecen con la barra reducida a íconos. Con la barra abierta siguen a mano en «Más carpetas».</p>
              <div class="folder-list">
                {#each app.folders[a.id] ?? [] as f (f.id)}
                  {@const Icon = FOLDER_ICONS[f.role]}
                  <label class="folder" class:off={app.isHidden(f)} title={f.role === 'inbox' ? 'Recibidos siempre se muestra' : f.path}>
                    <input type="checkbox" checked={!app.isHidden(f)} disabled={f.role === 'inbox'} onchange={() => app.toggleHidden(f)} />
                    <Icon size={15} />
                    <span>{f.name}</span>
                  </label>
                {/each}
              </div>
            </details>
          {/if}
        {/each}
        <button class="btn" onclick={() => { app.showSettings = null; app.showWizard = true; }}><Plus size={16} /> Agregar cuenta</button>
        <p class="faint small note">🔒 Las contraseñas y tokens se guardan en el llavero del sistema operativo, nunca en archivos de Cornamusa.</p>
      {:else if tab === 'signatures'}
        <h2>Firmas</h2>
        <SignatureEditor />
      {/if}
    </section>
  </div>
</div>

<style>
  .settings { width: min(920px, calc(100vw - 48px)); height: min(680px, calc(100vh - 48px)); flex-direction: row; overflow: hidden; }
  /* Signatures are usually wide (logo + details side by side): give the editor room. */
  .settings.wide { width: min(1180px, calc(100vw - 48px)); }
  nav { width: 200px; background: var(--surface-2); padding: 16px 10px; display: flex; flex-direction: column; gap: 2px; border-right: 1px solid var(--border); }
  nav h3 { margin: 4px 10px 12px; font-size: 15px; }
  nav button { display: flex; align-items: center; gap: 9px; padding: 8px 10px; border: none; background: none; border-radius: 8px; color: var(--text-2); text-align: left; }
  nav button:hover { background: var(--surface-3); }
  nav button.active { background: var(--accent-soft); color: var(--accent); font-weight: 600; }
  section { flex: 1; overflow-y: auto; padding: 22px 28px; position: relative; }
  .close { position: absolute; right: 14px; top: 14px; }
  h2 { margin: 0 0 18px; font-size: 20px; }
  .group { border: 1px solid var(--border); border-radius: 12px; margin-bottom: 16px; }
  .opt { display: flex; align-items: center; justify-content: space-between; gap: 20px; padding: 14px 16px; border-bottom: 1px solid var(--border); }
  .opt:last-child { border-bottom: none; }
  .opt p { margin: 3px 0 0; color: var(--text-3); font-size: 12.5px; line-height: 1.4; }
  .opt.sound { align-items: flex-start; }
  .sounds { display: flex; flex-direction: column; gap: 2px; min-width: 210px; }
  .sound-row { display: flex; align-items: center; justify-content: space-between; gap: 8px; min-height: 30px; }
  .sound-row label { display: flex; align-items: center; gap: 8px; font-size: 13px; padding: 3px 0; cursor: pointer; }
  .sound-row .icon-btn { padding: 4px; }
  .sound-row label { min-width: 0; }
  .custom-name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: 220px; }
  .custom-actions { display: flex; gap: 2px; flex-shrink: 0; }
  .seg { display: flex; background: var(--surface-2); padding: 3px; border-radius: 9px; }
  .seg button { white-space: nowrap; border: none; background: none; padding: 5px 12px; border-radius: 7px; font-size: 13px; color: var(--text-2); }
  .seg button.on { background: var(--surface); color: var(--accent); font-weight: 600; box-shadow: var(--shadow); }
  .account { display: flex; align-items: center; gap: 14px; padding: 12px 14px; border: 1px solid var(--border); border-radius: 12px; margin-bottom: 10px; }
  .colors { display: grid; grid-template-columns: repeat(4, 14px); gap: 5px; }
  .colors button { width: 14px; height: 14px; border-radius: 50%; border: none; padding: 0; }
  .colors button.sel { box-shadow: 0 0 0 2px var(--surface), 0 0 0 4px currentColor; color: var(--text-2); }
  .info { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 4px; }
  .small { font-size: 12px; }
  .acct-sound { display: flex; align-items: center; gap: 6px; color: var(--text-3); flex-shrink: 0; }
  .acct-sound select { width: 200px; padding: 5px 8px; font-size: 12.5px; }
  .folders { margin: -4px 0 14px; padding: 0 14px; }
  .folders summary { cursor: pointer; font-size: 13px; color: var(--text-2); padding: 4px 0; }
  .folders p { margin: 4px 0 8px; }
  .folder-list { display: grid; grid-template-columns: repeat(auto-fill, minmax(190px, 1fr)); gap: 2px 12px; }
  .folder { display: flex; align-items: center; gap: 8px; font-size: 13px; padding: 4px 0; min-width: 0; }
  .folder span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .folder.off { color: var(--text-3); }
  .error { color: var(--danger); }
  .note { margin-top: 18px; }
</style>
