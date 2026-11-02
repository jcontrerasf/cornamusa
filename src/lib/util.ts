import type { EmailAddress } from './api';

const LOCALE = 'es';

export function formatListDate(ts: number): string {
  const d = new Date(ts * 1000);
  const now = new Date();
  if (d.toDateString() === now.toDateString()) {
    return d.toLocaleTimeString(LOCALE, { hour: '2-digit', minute: '2-digit' });
  }
  const yesterday = new Date(now);
  yesterday.setDate(now.getDate() - 1);
  if (d.toDateString() === yesterday.toDateString()) return 'Ayer';
  if (now.getTime() - d.getTime() < 6 * 86400_000) {
    return d.toLocaleDateString(LOCALE, { weekday: 'short' });
  }
  if (d.getFullYear() === now.getFullYear()) {
    return d.toLocaleDateString(LOCALE, { day: 'numeric', month: 'short' });
  }
  return d.toLocaleDateString(LOCALE, { day: '2-digit', month: '2-digit', year: '2-digit' });
}

export function formatFullDate(ts: number): string {
  return new Date(ts * 1000).toLocaleString(LOCALE, {
    weekday: 'short', day: 'numeric', month: 'short', year: 'numeric', hour: '2-digit', minute: '2-digit',
  });
}

export function formatTime(ts: number): string {
  return new Date(ts * 1000).toLocaleTimeString(LOCALE, { hour: '2-digit', minute: '2-digit' });
}

export function formatDay(ts: number): string {
  const d = new Date(ts * 1000);
  const now = new Date();
  if (d.toDateString() === now.toDateString()) return 'Hoy';
  const y = new Date(now);
  y.setDate(now.getDate() - 1);
  if (d.toDateString() === y.toDateString()) return 'Ayer';
  return d.toLocaleDateString(LOCALE, {
    weekday: 'long', day: 'numeric', month: 'long', year: d.getFullYear() === now.getFullYear() ? undefined : 'numeric',
  });
}

export function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

export function displayName(a: EmailAddress | null | undefined): string {
  if (!a) return '(desconocido)';
  return a.name?.trim() || a.email;
}

export function firstName(a: EmailAddress | null | undefined): string {
  const n = displayName(a);
  return n.includes('@') ? n.split('@')[0] : n.split(/\s+/)[0];
}

export function initials(a: EmailAddress | null | undefined): string {
  const n = displayName(a).replace(/[^\p{L}\p{N}\s@.]/gu, '').trim();
  if (!n) return '?';
  if (n.includes('@')) return n[0].toUpperCase();
  const parts = n.split(/\s+/);
  return ((parts[0]?.[0] ?? '') + (parts.length > 1 ? parts[parts.length - 1][0] : '')).toUpperCase();
}

const AVATAR_COLORS = ['#6366f1', '#ec4899', '#14b8a6', '#f59e0b', '#8b5cf6', '#ef4444', '#22c55e', '#0ea5e9', '#f97316', '#84cc16'];

export function colorFor(key: string): string {
  let h = 0;
  for (const c of key.toLowerCase()) h = (h * 31 + c.charCodeAt(0)) >>> 0;
  return AVATAR_COLORS[h % AVATAR_COLORS.length];
}

export function escapeHtml(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
}

export function textToHtml(s: string): string {
  return escapeHtml(s).replace(/\r?\n/g, '<br>');
}

/** Parses "Ana <ana@x.com>, bob@y.com" into addresses. */
export function parseAddresses(input: string): EmailAddress[] {
  const out: EmailAddress[] = [];
  for (const raw of input.split(/[,;\n]/)) {
    const part = raw.trim();
    if (!part) continue;
    const m = part.match(/^\s*"?([^"<]*?)"?\s*<([^>]+)>\s*$/);
    if (m) out.push({ name: m[1].trim() || null, email: m[2].trim() });
    else out.push({ name: null, email: part });
  }
  return out;
}

export function isValidEmail(e: string): boolean {
  return /^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(e.trim());
}

export function sameEmail(a: string, b: string): boolean {
  return a.trim().toLowerCase() === b.trim().toLowerCase();
}

export function prefixSubject(prefix: 'Re' | 'Fwd', subject: string): string {
  const re = prefix === 'Re' ? /^\s*(re|res|rv)\s*:/i : /^\s*(fwd?|rv|reenv)\s*:/i;
  return re.test(subject) ? subject : `${prefix}: ${subject}`;
}

// ---------- dark rendering of emails ----------

const NEUTRAL_BG = /^(#fff|#ffffff|white|transparent|none|initial|inherit|unset|rgba?\(\s*255\s*,\s*255\s*,\s*255\s*(,\s*(1|1\.0*|0?\.\d+|0)\s*)?\)|rgba\([^)]*,\s*0\s*\)|repeat|no-repeat|repeat-x|repeat-y|scroll|fixed|local)$/i;
const BG_DECL = /background(?:-color|-image)?\s*:\s*([^;}"]+)/gi;

/** True when a CSS background value is white/transparent (Gmail pastes `background-color:rgb(255,255,255)`). */
function neutralBackground(value: string): boolean {
  const v = value.replace(/!important/i, '').trim();
  if (/url\(|gradient\(/i.test(v)) return false;
  const tokens = v.match(/[a-z-]+\([^)]*\)|[^\s,]+/gi) ?? [];
  return tokens.every((t) => NEUTRAL_BG.test(t));
}

function cssHasColoredBackground(css: string): boolean {
  for (const m of css.matchAll(BG_DECL)) if (!neutralBackground(m[1])) return true;
  return false;
}

/**
 * Heuristic: can this email be shown on a dark background without breaking its design?
 * Person-to-person mail (plain text, Gmail/Outlook replies) usually can; newsletters with
 * colored backgrounds or fixed-width layout tables can't. Text colors don't disqualify an
 * email: they are lightened after rendering (see `darkTextColor`).
 */
export function looksDarkSafe(html: string | null | undefined): boolean {
  if (!html) return true;
  const doc = new DOMParser().parseFromString(html, 'text/html');
  if (doc.querySelector('[background]')) return false;
  for (const el of doc.querySelectorAll('[bgcolor]')) {
    if (!neutralBackground(el.getAttribute('bgcolor') ?? '')) return false;
  }
  for (const el of doc.querySelectorAll('[style]')) {
    if (cssHasColoredBackground(el.getAttribute('style') ?? '')) return false;
  }
  for (const el of doc.querySelectorAll('style')) {
    if (cssHasColoredBackground(el.textContent ?? '')) return false;
  }
  for (const t of doc.querySelectorAll('table')) {
    const w = parseInt(t.getAttribute('width') ?? '', 10) || parseInt(/(?:^|;)\s*(?:max-)?width\s*:\s*(\d+)px/i.exec(t.getAttribute('style') ?? '')?.[1] ?? '', 10);
    if (w >= 480) return false;
  }
  return true;
}

function parseRgb(color: string): [number, number, number, number] | null {
  const m = /rgba?\(\s*([\d.]+)[,\s]+([\d.]+)[,\s]+([\d.]+)(?:\s*[,/]\s*([\d.]+%?))?\s*\)/.exec(color);
  if (!m) return null;
  const a = m[4] === undefined ? 1 : m[4].endsWith('%') ? parseFloat(m[4]) / 100 : parseFloat(m[4]);
  return [+m[1], +m[2], +m[3], a];
}

function luminance([r, g, b]: number[]): number {
  const f = (c: number) => ((c /= 255) <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4);
  return 0.2126 * f(r) + 0.7152 * f(g) + 0.0722 * f(b);
}

/**
 * For a computed text color that would be hard to read on a dark background, returns a
 * readable replacement (keeping the hue of colored text, e.g. Outlook's #1F497D), or null
 * when the color is already fine.
 */
export function darkTextColor(color: string, fallback: string): string | null {
  const c = parseRgb(color);
  if (!c || c[3] === 0 || luminance(c) > 0.18) return null;
  const [r, g, b] = c.map((v) => v / 255);
  const max = Math.max(r, g, b);
  const min = Math.min(r, g, b);
  const sat = max === 0 ? 0 : (max - min) / max;
  if (sat < 0.25) return fallback; // black/gray text → theme text color
  let h = 0;
  if (max !== min) {
    const d = max - min;
    h = max === r ? ((g - b) / d) % 6 : max === g ? (b - r) / d + 2 : (r - g) / d + 4;
  }
  return `hsl(${Math.round(((h * 60) + 360) % 360)} 70% 75%)`;
}

/** True for an opaque, near-white background (left over from pasted text). */
export function isWhiteBackground(color: string): boolean {
  const c = parseRgb(color);
  return !!c && c[3] > 0 && luminance(c) > 0.85;
}
