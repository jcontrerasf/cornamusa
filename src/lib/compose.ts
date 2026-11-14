// Helpers shared by the full composer and the chat quick-reply box.
import type { Account, EmailAddress, MessageBody, Signature } from './api';
import type { ComposeInit } from './state.svelte';
import { displayName, escapeHtml, formatFullDate, prefixSubject, sameEmail, textToHtml } from './util';

export function htmlToText(html: string): string {
  const doc = new DOMParser().parseFromString(html, 'text/html');
  doc.querySelectorAll('br').forEach((br) => br.replaceWith('\n'));
  doc.querySelectorAll('p, div, li, tr, h1, h2, h3, h4, blockquote').forEach((el) => el.append('\n'));
  return (doc.body.textContent ?? '').replace(/\n{3,}/g, '\n\n').trim();
}

export function signatureHtml(sig: Signature | undefined): string {
  if (!sig || !sig.body.trim()) return '';
  return sig.isHtml ? sig.body : textToHtml(sig.body);
}

export function signatureText(sig: Signature | undefined): string {
  if (!sig || !sig.body.trim()) return '';
  return sig.isHtml ? htmlToText(sig.body) : sig.body;
}

function dedupe(list: EmailAddress[], exclude: string[]): EmailAddress[] {
  const out: EmailAddress[] = [];
  for (const a of list) {
    if (exclude.some((e) => sameEmail(e, a.email))) continue;
    if (out.some((o) => sameEmail(o.email, a.email))) continue;
    out.push(a);
  }
  return out;
}

export function replyRecipients(msg: MessageBody, account: Account, all: boolean): { to: EmailAddress[]; cc: EmailAddress[] } {
  const s = msg.summary;
  const fromMe = s.from && sameEmail(s.from.email, account.email);
  // Replying to our own message continues the conversation with its recipients.
  const primary = fromMe ? s.to : msg.replyTo.length ? msg.replyTo : s.from ? [s.from] : [];
  if (!all) return { to: dedupe(primary, fromMe ? [] : [account.email]), cc: [] };
  const to = dedupe([...primary, ...(fromMe ? [] : s.to)], [account.email]);
  const cc = dedupe(s.cc, [account.email, ...to.map((t) => t.email)]);
  return { to, cc };
}

export function threadingHeaders(msg: MessageBody): { inReplyTo: string | null; references: string[] } {
  const id = msg.summary.messageId;
  const refs = [...msg.references];
  if (id && !refs.includes(id)) refs.push(id);
  return { inReplyTo: id, references: refs };
}

export function replySubject(msg: MessageBody): string {
  return prefixSubject('Re', msg.summary.subject);
}

export function forwardSubject(msg: MessageBody): string {
  return prefixSubject('Fwd', msg.summary.subject);
}

function sourceHtml(msg: MessageBody): string {
  return msg.html ?? textToHtml(msg.text ?? '');
}

export function quoteHeader(msg: MessageBody): string {
  const from = msg.summary.from;
  return `El ${formatFullDate(msg.summary.date)}, ${displayName(from)}${from && from.name ? ` <${from.email}>` : ''} escribió:`;
}

export function quotedHtml(msg: MessageBody): string {
  return `<div class="cornamusa-quote"><p>${escapeHtml(quoteHeader(msg))}</p>` +
    `<blockquote style="margin:0 0 0 .8ex;border-left:2px solid #ccc;padding-left:1ex">${sourceHtml(msg)}</blockquote></div>`;
}

export function quotedText(msg: MessageBody): string {
  const body = msg.text ?? htmlToText(msg.html ?? '');
  return `${quoteHeader(msg)}\n` + body.split('\n').map((l) => `> ${l}`).join('\n');
}

export function forwardedHtml(msg: MessageBody): string {
  const s = msg.summary;
  const list = (l: EmailAddress[]) => l.map((a) => escapeHtml(a.name ? `${a.name} <${a.email}>` : a.email)).join(', ');
  return `<div class="cornamusa-forward"><p>---------- Mensaje reenviado ----------<br>` +
    `De: ${s.from ? list([s.from]) : ''}<br>Fecha: ${escapeHtml(formatFullDate(s.date))}<br>` +
    `Asunto: ${escapeHtml(s.subject)}<br>Para: ${list(s.to)}${s.cc.length ? `<br>CC: ${list(s.cc)}` : ''}</p>` +
    `${sourceHtml(msg)}</div>`;
}

export function forwardedText(msg: MessageBody): string {
  const s = msg.summary;
  const list = (l: EmailAddress[]) => l.map((a) => (a.name ? `${a.name} <${a.email}>` : a.email)).join(', ');
  return `---------- Mensaje reenviado ----------\nDe: ${s.from ? list([s.from]) : ''}\nFecha: ${formatFullDate(s.date)}\n` +
    `Asunto: ${s.subject}\nPara: ${list(s.to)}\n\n${msg.text ?? htmlToText(msg.html ?? '')}`;
}

export function linkify(escaped: string): string {
  return escaped.replace(/\bhttps?:\/\/[^\s<>"']+[^\s<>"'.,;:!?)\]]/g, (u) => `<a href="${u}">${u}</a>`);
}

export interface DraftStart {
  accountId: string;
  to: EmailAddress[];
  cc: EmailAddress[];
  subject: string;
  content: string;
  contentText: string;
  quoteHtml: string;
  quoteText: string;
  inReplyTo: string | null;
  references: string[];
  forwardOf: number | null;
  draftId: string | null;
}

/** Initial composer state for new messages, replies, forwards and reopened drafts. */
export function initialDraft(init: ComposeInit, account: Account | undefined, fallbackAccountId: string): DraftStart {
  const src = init.source;
  const start: DraftStart = {
    accountId: init.accountId ?? src?.summary.accountId ?? fallbackAccountId,
    to: init.to ?? [], cc: [], subject: '', content: '', contentText: '', quoteHtml: '', quoteText: '',
    inReplyTo: null, references: [], forwardOf: null, draftId: null,
  };
  if (!src || !account) return withOverrides(init, start);
  const th = threadingHeaders(src);
  if (init.mode === 'reply' || init.mode === 'replyAll') {
    const r = replyRecipients(src, account, init.mode === 'replyAll');
    Object.assign(start, {
      to: r.to, cc: r.cc, subject: replySubject(src), inReplyTo: th.inReplyTo, references: th.references,
      quoteHtml: quotedHtml(src), quoteText: quotedText(src),
    });
  } else if (init.mode === 'forward') {
    Object.assign(start, {
      subject: forwardSubject(src), quoteHtml: forwardedHtml(src), quoteText: forwardedText(src), references: th.references,
      forwardOf: src.attachments.some((a) => !a.inline) ? src.summary.id : null,
    });
  } else if (init.mode === 'draft') {
    Object.assign(start, {
      to: src.summary.to, cc: src.summary.cc, subject: src.summary.subject,
      content: src.html ?? textToHtml(src.text ?? ''), contentText: src.text ?? '',
      draftId: src.summary.messageId, inReplyTo: src.inReplyTo, references: src.references,
    });
  }
  return withOverrides(init, start);
}

/** Explicit values (drafts prepared by Claude) win over the ones derived from the source. */
function withOverrides(init: ComposeInit, start: DraftStart): DraftStart {
  if (init.to !== undefined) start.to = init.to;
  if (init.cc !== undefined) start.cc = init.cc;
  if (init.subject !== undefined) start.subject = init.subject;
  if (init.body !== undefined) {
    start.content = init.body;
    start.contentText = htmlToText(init.body);
  }
  if (init.draftId) start.draftId = init.draftId;
  return start;
}
