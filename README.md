# Cornamusa

Cliente de correo de escritorio para **Linux, Windows y macOS**, escrito en **Rust** (Tauri 2 + Svelte 5).

Su función principal es un **toggle para ver los hilos como chat**: cada correo es una burbuja, sin el
historial citado ni las firmas que se repiten. Así los hilos largos se leen como una conversación.

## Funcionalidades

- **Multicuenta**: Gmail y Outlook / Hotmail / Microsoft 365 con OAuth 2.0, y cualquier servidor IMAP/SMTP
  (Yahoo, iCloud, Fastmail, servidor propio…) con autodetección de servidores.
- **Bandejas unificadas** ("Todas las cuentas") y por cuenta: Recibidos, Enviados, Borradores, Archivo,
  No deseado, Papelera, más tus carpetas.
- **Vista clásica / vista chat** con un toggle (tecla `V`). La vista chat tiene separadores por día, agrupa
  los mensajes consecutivos del mismo remitente y trae una caja de respuesta rápida (responde a todos).
- **Hilos reales**: usa `Message-ID`/`In-Reply-To`/`References`, junta tus respuestas de Enviados con las
  de Recibidos y tolera clientes que omiten `References` (Outlook) o correos que llegan desordenados.
- **Redactar**: editor enriquecido, Para/CC/CCO con autocompletado, adjuntos (diálogo o arrastrar y soltar),
  responder / responder a todos / reenviar (con los adjuntos originales) y borradores autoguardados en el
  servidor.
- **Firmas**: varias por cuenta, en **texto** o **HTML**, con editor visual o de código y vista previa.
- **Notificaciones** en tiempo real con IMAP IDLE, y modo "No molestar".
- **Bandeja del sistema**: al cerrar la ventana la app sigue en la bandeja (se puede desactivar). El ícono
  muestra los no leídos y tiene un menú con Abrir, Redactar, Buscar correo nuevo, No molestar y Salir.
  También puede iniciar con el sistema.
- Búsqueda local instantánea (SQLite FTS5, ignora tildes), tema claro/oscuro y atajos de teclado.

| Atajo | Acción | Atajo | Acción |
|---|---|---|---|
| `C` | Redactar | `E` | Archivar |
| `R` / `A` / `F` | Responder / a todos / reenviar | `#` / `Supr` | Eliminar |
| `J` / `K` | Siguiente / anterior | `S` | Destacar |
| `/` | Buscar | `U` | Marcar no leído |
| `V` | Alternar vista chat | `Ctrl+Enter` | Enviar |

## Seguridad

1. **OAuth 2.0 con PKCE** para Gmail y Microsoft. Ambos desactivaron la autenticación con contraseña en
   IMAP/SMTP. El inicio de sesión ocurre en tu navegador, en la página oficial del proveedor, con redirección
   a `127.0.0.1` (RFC 8252). Cornamusa nunca ve tu contraseña y el `state` se valida contra CSRF.
2. **Credenciales en el llavero del sistema** (Keychain en macOS, Credential Manager en Windows, Secret
   Service/libsecret en Linux). Los *refresh tokens* y las contraseñas nunca se guardan en la base de datos
   ni en archivos. Los *access tokens* solo viven en memoria.
3. **TLS obligatorio**: IMAPS/SMTPS o STARTTLS *requerido*. Si el servidor no ofrece TLS la conexión falla;
   nunca se cae a texto plano. Se verifican los certificados con las raíces de Mozilla y las del sistema
   operativo.
4. **El HTML de los correos se trata como hostil**:
   - Se sanitiza en Rust con [ammonia]: sin `<script>`, `on*=`, `javascript:`, `<form>`, `<iframe>`,
     `<object>`, CSS `expression()` ni `@import`.
   - Se muestra en un `<iframe sandbox>` **sin** `allow-scripts` y con una CSP propia que bloquea toda
     conexión de red.
   - **Las imágenes remotas se bloquean por defecto** (píxeles de rastreo). Puedes mostrarlas para un
     correo o confiar siempre en un remitente.
   - Al pasar el mouse sobre un enlace se ve su **URL real** (anti-phishing) y se abre en el navegador.
     Solo se permiten `http`/`https`.
5. **Anti-phishing**: siempre se muestra la dirección real del remitente junto a su nombre.
6. **Adjuntos**: nunca se abren automáticamente. Se guardan donde elijas, con el nombre saneado (sin rutas,
   `..` ni nombres reservados de Windows).
7. **Permisos mínimos en Tauri**: CSP estricta y *capabilities* mínimas. Diálogos, navegador y
   notificaciones se manejan desde Rust, no desde JavaScript.
8. **Caché local**: la base SQLite tiene permisos `0600` en Unix y está en el directorio de datos del
   usuario.

> **Aviso sobre Gmail**: el permiso `https://mail.google.com/` es "restringido". Mientras tu proyecto de
> Google Cloud esté en modo *Testing* solo pueden usarlo hasta 100 usuarios de prueba que agregues tú.
> Para publicarlo para cualquiera, Google exige verificación de la app y una auditoría de seguridad (CASA).
> En Microsoft 365 el administrador del tenant puede tener desactivado *SMTP AUTH*; en ese caso hay que
> habilitarlo para el buzón.

## Configurar OAuth

Los *client IDs* no se incluyen en el código. Puedes darlos al ejecutar, por ejemplo con `.env`
(ver `.env.example`), o al compilar. Si se definen al compilar quedan incrustados en el binario.

### Google (Gmail)

1. En [Google Cloud Console](https://console.cloud.google.com/), crea un proyecto y habilita **Gmail API**.
2. *Pantalla de consentimiento de OAuth* → tipo **Externo**. Agrega el scope `https://mail.google.com/` y,
   mientras estés en *Testing*, tus cuentas como usuarios de prueba.
3. *Credenciales* → *Crear ID de cliente OAuth* → tipo **App de escritorio**.
4. Copia el ID y el secreto en `CORNAMUSA_GOOGLE_CLIENT_ID` y `CORNAMUSA_GOOGLE_CLIENT_SECRET`. En apps de
   escritorio ese secreto no es confidencial: la seguridad la da PKCE.

### Microsoft (Outlook, Hotmail, Microsoft 365)

> **Necesitas un directorio (tenant) de Microsoft Entra para registrar la app.** Una cuenta personal
> (`@outlook.com`, `@hotmail.com`) sola no puede entrar al centro de administración. Microsoft responde:
> *"La cuenta de usuario seleccionada no existe en el inquilino 'Microsoft Services'"*. Para obtener uno:
> crea una cuenta gratuita de Azure (azure.microsoft.com/free, crea un tenant "Directorio predeterminado"),
> usa una cuenta de trabajo/estudio con permiso para registrar apps, o el Microsoft 365 Developer Program
> si eres elegible. El registro puede vivir en cualquier tenant: si eliges el tipo de cuenta del paso 1,
> igual sirve para iniciar sesión con cuentas personales de Hotmail/Outlook.

1. En [Microsoft Entra](https://entra.microsoft.com/) → *Registros de aplicaciones* → *Nuevo registro*.
   - Tipos de cuenta: **cuentas de cualquier directorio y cuentas personales de Microsoft**.
   - URI de redirección: plataforma **Aplicaciones móviles y de escritorio**, `http://localhost`.
2. *Autenticación* → activa **Permitir flujos de cliente público**.
3. *Permisos de API* → *Office 365 Exchange Online* (o *APIs que usa mi organización* → "Office 365 Exchange
   Online") → permisos delegados `IMAP.AccessAsUser.All` y `SMTP.Send`. Agrega también `offline_access`,
   `openid`, `email` y `profile`.
4. Copia el *ID de aplicación (cliente)* en `CORNAMUSA_MS_CLIENT_ID`.

## Desarrollo

Requisitos: Rust estable (≥ 1.88), Node.js ≥ 20, y en Linux:

```sh
# Debian/Ubuntu
sudo apt install libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libsecret-1-dev
# Fedora
sudo dnf install webkit2gtk4.1-devel libayatana-appindicator-gtk3-devel librsvg2-devel libsecret-devel
```

En GNOME, el ícono de la bandeja necesita la extensión *AppIndicator and KStatusNotifierItem Support*.
Windows (WebView2) y macOS no necesitan nada más.

```sh
npm install
set -a; . ./.env; set +a      # client IDs de OAuth
npm run tauri dev             # app en modo desarrollo
npm run tauri build           # instaladores: .deb/.rpm/.AppImage, .msi/.exe, .dmg/.app
```

La CI (`.github/workflows/build.yml`) corre las pruebas y genera los instaladores de las tres plataformas.
Los client IDs se configuran como *secrets* del repositorio.

## Pruebas

```sh
cargo test -p cornamusa-core      # unitarias: sanitizado/XSS, recorte de citas, hilos, MIME, SQLite, OAuth/PKCE…
npm run check                     # tipos del frontend (svelte-check)
cargo clippy --workspace --all-targets
```

**Integración end-to-end** contra un servidor IMAP/SMTP real con TLS ([GreenMail]). El flujo completo está
en `crates/core/tests/greenmail.rs`: alta de cuentas, sincronización, adjuntos, respuestas que forman un
hilo entre Recibidos y Enviados, flags, archivar, borradores, búsqueda, XOAUTH2, rechazo de texto plano y
notificación push por IDLE. Para correrlo localmente, mira cómo se levanta GreenMail en el job `test` de la
CI y luego ejecuta:

```sh
CORNAMUSA_IT=1 CORNAMUSA_EXTRA_CA=/ruta/ca.pem cargo test -p cornamusa-core --test greenmail
```

## Arquitectura

```
crates/core/          lógica de correo, independiente de la UI (testeable sin ventana)
  src/imap.rs         TLS/STARTTLS, XOAUTH2/LOGIN, carpetas SPECIAL-USE, fetch, MOVE, IDLE
  src/smtp.rs         envío (lettre) con TLS obligatorio
  src/oauth.rs        Authorization Code + PKCE + loopback
  src/secrets.rs      llavero del sistema (keyring)
  src/store.rs        caché SQLite + FTS5, hilos, contactos, firmas, ajustes
  src/service.rs      orquestación: un worker por cuenta (sync incremental + IDLE), acciones
  src/mime.rs         parseo (mail-parser) y construcción (lettre) de mensajes
  src/sanitize.rs     sanitizado de HTML y bloqueo de contenido remoto
  src/quote_strip.rs  extrae la parte nueva de cada correo para la vista chat
  src/threading.rs    Message-ID / References
src-tauri/            app de escritorio: comandos, bandeja del sistema, notificaciones
src/                  interfaz Svelte 5
```

## Limitaciones conocidas

- Hacer clic en una notificación no abre ese mensaje: el plugin de notificaciones de Tauri no expone el clic
  en escritorio. El clic en el ícono de la bandeja sí restaura la ventana.
- La búsqueda es sobre la caché local (por defecto los últimos ~400 mensajes por carpeta especial, más los
  que cargues desplazándote hacia abajo).
- Gmail: los hilos se calculan con `References`, no con `X-GM-THRID`.

[ammonia]: https://github.com/rust-ammonia/ammonia
[GreenMail]: https://greenmail-mail-test.github.io/greenmail/
