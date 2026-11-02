<script lang="ts">
  import { app } from '../state.svelte';
  import { api, type DetectedConfig, type Provider, type Security } from '../api';
  import { X, ArrowLeft, Lock, ExternalLink, ChevronDown, ChevronRight } from '@lucide/svelte';

  type Step = 'choose' | 'oauth' | 'imap';
  let step = $state<Step>('choose');
  let provider = $state<Provider>('gmail');
  let busy = $state(false);
  let error = $state<string | null>(null);

  let hint = $state('');
  // Microsoft 365 shared mailbox: sign in with your own user, access the shared address.
  let shared = $state(false);
  let sharedEmail = $state('');

  let email = $state('');
  let name = $state('');
  let password = $state('');
  let username = $state('');
  let imapHost = $state('');
  let imapPort = $state(993);
  let imapSec = $state<Security>('tls');
  let smtpHost = $state('');
  let smtpPort = $state(465);
  let smtpSec = $state<Security>('tls');
  let appHint = $state<string | null>(null);
  let advanced = $state(false);
  let detectedFor = '';

  function choose(p: Provider) {
    provider = p;
    error = null;
    step = p === 'imap' ? 'imap' : 'oauth';
    shared = false;
  }

  function toggleShared() {
    shared = !shared;
    error = null;
    // Prefill "your account" with an Outlook account that's already set up.
    if (shared && !hint.trim()) {
      hint = app.accounts.find((a) => a.provider === 'outlook' && !a.authUser)?.email ?? '';
    }
  }

  async function startOAuth() {
    busy = true;
    error = null;
    try {
      const account = shared
        ? await api.addSharedMailbox(sharedEmail.trim(), hint.trim() || null)
        : await api.addOAuthAccount(provider, hint.trim() || null);
      app.toast(shared ? `Buzón compartido ${account.email} conectado` : `Cuenta ${account.email} conectada`, 'success');
      app.showWizard = false;
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  async function detect() {
    const e = email.trim();
    if (!e.includes('@') || e === detectedFor) return;
    detectedFor = e;
    const cfg: DetectedConfig = await api.detectConfig(e);
    if (cfg.provider !== 'imap') {
      // Gmail/Outlook addresses must use OAuth: basic auth is disabled by both providers.
      error = `Esta dirección es de ${cfg.provider === 'gmail' ? 'Gmail' : 'Outlook'}: usa la opción de inicio de sesión seguro.`;
      return;
    }
    error = null;
    imapHost = cfg.imap.host; imapPort = cfg.imap.port; imapSec = cfg.imap.security;
    smtpHost = cfg.smtp.host; smtpPort = cfg.smtp.port; smtpSec = cfg.smtp.security;
    appHint = cfg.appPasswordHint;
  }

  async function connectImap(e: Event) {
    e.preventDefault();
    await detect();
    if (error) return;
    busy = true;
    try {
      const account = await api.addPasswordAccount({
        email: email.trim(), displayName: name.trim(), username: username.trim() || null, password,
        imap: { host: imapHost.trim(), port: Number(imapPort), security: imapSec },
        smtp: { host: smtpHost.trim(), port: Number(smtpPort), security: smtpSec },
      });
      app.toast(`Cuenta ${account.email} conectada`, 'success');
      app.showWizard = false;
    } catch (err) {
      error = String(err);
      advanced = true;
    } finally {
      busy = false;
    }
  }

  const canClose = $derived(app.accounts.length > 0);
</script>

<div class="overlay" role="presentation" onclick={(e) => canClose && e.target === e.currentTarget && !busy && (app.showWizard = false)}>
  <div class="modal wizard" role="dialog" aria-label="Agregar cuenta">
    <header>
      {#if step !== 'choose'}<button class="icon-btn" onclick={() => { step = 'choose'; error = null; }} disabled={busy}><ArrowLeft size={18} /></button>{/if}
      <h2>{step === 'choose' ? (canClose ? 'Agregar cuenta' : 'Bienvenido a Cornamusa') : step === 'imap' ? 'Otra cuenta de correo' : provider === 'gmail' ? 'Gmail' : 'Outlook / Hotmail / Microsoft 365'}</h2>
      {#if canClose}<button class="icon-btn" onclick={() => (app.showWizard = false)} disabled={busy}><X size={18} /></button>{/if}
    </header>

    <div class="content">
      {#if step === 'choose'}
        <p class="muted">Elige tu proveedor de correo.</p>
        <div class="providers">
          <button class="prov" onclick={() => choose('gmail')}>
            <span class="logo g">G</span><strong>Gmail</strong><span class="faint">Google Workspace incluido</span>
          </button>
          <button class="prov" onclick={() => choose('outlook')}>
            <span class="logo o">O</span><strong>Outlook</strong><span class="faint">Hotmail, Live, Microsoft 365</span>
          </button>
          <button class="prov" onclick={() => choose('imap')}>
            <span class="logo i">@</span><strong>Otra cuenta</strong><span class="faint">IMAP / SMTP (Yahoo, iCloud, Fastmail…)</span>
          </button>
        </div>
      {:else if step === 'oauth'}
        <div class="secure">
          <Lock size={18} />
          <div>
            <strong>Inicio de sesión seguro (OAuth 2.0)</strong>
            <p>Se abrirá tu navegador en la página oficial de {provider === 'gmail' ? 'Google' : 'Microsoft'}. Cornamusa nunca ve tu contraseña; solo recibe un permiso revocable que se guarda en el llavero del sistema.</p>
          </div>
        </div>
        {#if shared}
          <label class="field">Dirección del buzón compartido
            <input class="input" type="email" bind:value={sharedEmail} placeholder="ventas@tuempresa.cl" disabled={busy} />
          </label>
          <label class="field">Tu cuenta (con la que inicias sesión)
            <input class="input" type="email" bind:value={hint} placeholder="tu@tuempresa.cl" disabled={busy} />
          </label>
          <p class="faint small">Necesitas el permiso <em>Full Access</em> sobre el buzón para leerlo y <em>Send As</em> para enviar desde él; los asigna el administrador de Microsoft 365.</p>
        {:else}
          <label class="field">Correo (opcional)
            <input class="input" type="email" bind:value={hint} placeholder={provider === 'gmail' ? 'tu@gmail.com' : 'tu@outlook.com'} disabled={busy} />
          </label>
        {/if}
        <button class="btn primary big" onclick={startOAuth} disabled={busy || (shared && !sharedEmail.includes('@'))}>
          <ExternalLink size={16} />
          {busy ? 'Esperando autorización en el navegador…' : `Continuar con ${provider === 'gmail' ? 'Google' : 'Microsoft'}`}
        </button>
        {#if busy}<p class="faint small">Completa el inicio de sesión en el navegador y vuelve aquí.</p>{/if}
        {#if provider === 'outlook' && !busy}
          <button type="button" class="adv" onclick={toggleShared}>
            {shared ? 'Agregar mi propia cuenta en su lugar' : '¿Buzón compartido de la empresa? Agrégalo aquí'}
          </button>
        {/if}
      {:else}
        <form onsubmit={connectImap}>
          <div class="grid">
            <label class="field">Tu nombre<input class="input" bind:value={name} placeholder="Ana Pérez" /></label>
            <label class="field">Correo<input class="input" type="email" bind:value={email} onblur={detect} required placeholder="ana@midominio.cl" /></label>
          </div>
          <label class="field">Contraseña<input class="input" type="password" bind:value={password} required autocomplete="off" /></label>
          {#if appHint}<p class="hint">💡 {appHint}</p>{/if}

          <button type="button" class="adv" onclick={() => (advanced = !advanced)}>
            {#if advanced}<ChevronDown size={14} />{:else}<ChevronRight size={14} />{/if} Configuración de servidores
          </button>
          {#if advanced}
            <div class="servers">
              <label class="field">Usuario (si no es el correo)<input class="input" bind:value={username} placeholder={email} /></label>
              <div class="srv">
                <span class="tag">IMAP</span>
                <input class="input" bind:value={imapHost} placeholder="imap.servidor.com" required />
                <input class="input port" type="number" bind:value={imapPort} required />
                <select class="input sec" bind:value={imapSec}><option value="tls">SSL/TLS</option><option value="starttls">STARTTLS</option></select>
              </div>
              <div class="srv">
                <span class="tag">SMTP</span>
                <input class="input" bind:value={smtpHost} placeholder="smtp.servidor.com" required />
                <input class="input port" type="number" bind:value={smtpPort} required />
                <select class="input sec" bind:value={smtpSec}><option value="tls">SSL/TLS</option><option value="starttls">STARTTLS</option></select>
              </div>
              <p class="faint small">Solo se permiten conexiones cifradas. Cornamusa nunca envía tu contraseña sin TLS.</p>
            </div>
          {/if}
          <button class="btn primary big" type="submit" disabled={busy}>{busy ? 'Verificando…' : 'Conectar'}</button>
        </form>
      {/if}

      {#if error}<div class="error">{error}</div>{/if}
    </div>
  </div>
</div>

<style>
  .wizard { width: min(620px, calc(100vw - 48px)); }
  header { display: flex; align-items: center; gap: 8px; padding: 16px 18px 4px 24px; }
  h2 { flex: 1; margin: 0; font-size: 19px; }
  .content { padding: 8px 24px 24px; display: flex; flex-direction: column; gap: 14px; overflow-y: auto; }
  .providers { display: grid; grid-template-columns: repeat(3, 1fr); gap: 12px; }
  .prov { display: flex; flex-direction: column; align-items: center; gap: 6px; padding: 20px 12px; border-radius: 14px; border: 1px solid var(--border); background: var(--surface); text-align: center; transition: border-color 0.12s, transform 0.12s; }
  .prov:hover { border-color: var(--accent); transform: translateY(-2px); }
  .prov span.faint { font-size: 12px; }
  .logo { width: 46px; height: 46px; border-radius: 12px; display: grid; place-items: center; color: #fff; font-weight: 800; font-size: 22px; margin-bottom: 4px; }
  .logo.g { background: linear-gradient(135deg, #ea4335, #fbbc05); }
  .logo.o { background: linear-gradient(135deg, #0078d4, #28a8ea); }
  .logo.i { background: linear-gradient(135deg, #6366f1, #8b5cf6); }
  .secure { display: flex; gap: 12px; padding: 14px; border-radius: 12px; background: var(--accent-soft); color: var(--text); }
  .secure p { margin: 4px 0 0; font-size: 13px; color: var(--text-2); line-height: 1.45; }
  .big { justify-content: center; padding: 11px; font-size: 14px; }
  form { display: flex; flex-direction: column; gap: 12px; }
  .grid { display: grid; grid-template-columns: 1fr 1fr; gap: 12px; }
  .hint { margin: 0; font-size: 12.5px; color: var(--text-2); background: var(--surface-2); padding: 8px 10px; border-radius: 8px; }
  .adv { display: flex; align-items: center; gap: 4px; border: none; background: none; color: var(--text-2); padding: 0; font-size: 13px; }
  .big + .adv, .small + .adv { margin-top: 12px; }
  .servers { display: flex; flex-direction: column; gap: 10px; padding: 12px; border: 1px solid var(--border); border-radius: 10px; }
  .srv { display: flex; gap: 8px; align-items: center; }
  .tag { width: 44px; font-size: 12px; font-weight: 700; color: var(--text-3); }
  .port { width: 90px; }
  .sec { width: 130px; }
  .small { font-size: 12px; margin: 0; }
  .error { color: var(--danger); background: color-mix(in srgb, var(--danger) 10%, transparent); padding: 10px 12px; border-radius: 8px; font-size: 13px; white-space: pre-wrap; user-select: text; }
</style>
