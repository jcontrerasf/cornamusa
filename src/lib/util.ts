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
