// New-mail sounds bundled with the app (synthesized by assets/sounds/generate.py), plus the
// user's own file ('custom'), copied into the app's data folder by the backend.
import { api } from './api';

export const SOUNDS = [
  { id: 'corneta-suave', label: 'Corneta suave' },
  { id: 'corneta-cartero', label: 'Corneta de cartero' },
  { id: 'gaita-breve', label: 'Gaita breve' },
] as const;

const VOLUME = 0.8;

export const CUSTOM_SOUND = 'custom';

export function isBundledSound(id: string): boolean {
  return SOUNDS.some((s) => s.id === id);
}

/** Sounds the UI plays itself (as opposed to 'system' / 'none'). */
export function isPlayableSound(id: string): boolean {
  return id === CUSTOM_SOUND || isBundledSound(id);
}

/** Drops the cached custom sound after the user picks another file. */
export function invalidateCustomSound() {
  buffers.delete(CUSTOM_SOUND);
}

// Played through Web Audio rather than <audio>: on Linux, WebKitGTK's media element can't load
// from the app's custom scheme (tauri://) in packaged builds, so it fails silently there.
let ctx: AudioContext | null = null;
const buffers = new Map<string, Promise<AudioBuffer>>();

function context(): AudioContext | null {
  if (ctx) return ctx;
  const Ctor = window.AudioContext ?? (window as unknown as { webkitAudioContext?: typeof AudioContext }).webkitAudioContext;
  if (!Ctor) return null;
  ctx = new Ctor();
  return ctx;
}

function load(c: AudioContext, id: string): Promise<AudioBuffer> {
  let p = buffers.get(id);
  if (!p) {
    const bytes =
      id === CUSTOM_SOUND
        ? api.readCustomSound()
        : fetch(`/sounds/${id}.wav`).then((r) => {
            if (!r.ok) throw new Error(`HTTP ${r.status}`);
            return r.arrayBuffer();
          });
    p = bytes.then((data) => c.decodeAudioData(data));
    p.catch(() => buffers.delete(id)); // retry next time
    buffers.set(id, p);
  }
  return p;
}

// A context created without a user gesture may start suspended; resume it on the first interaction
// so sounds for mail arriving later can play.
if (typeof document !== 'undefined') {
  const unlock = () => {
    const c = context();
    if (c && c.state === 'suspended') c.resume().catch(() => {});
  };
  document.addEventListener('pointerdown', unlock, { once: true, capture: true });
  document.addEventListener('keydown', unlock, { once: true, capture: true });
}

/** Plays a bundled or the custom sound; silently ignores unknown ids and playback errors.
 *  Resolves to false if the sound couldn't be played. */
export async function playSound(id: string): Promise<boolean> {
  if (!isPlayableSound(id)) return false;
  try {
    const c = context();
    if (!c) throw new Error('Web Audio no disponible');
    if (c.state === 'suspended') await c.resume();
    const buffer = await load(c, id);
    const src = c.createBufferSource();
    const gain = c.createGain();
    gain.gain.value = VOLUME;
    src.buffer = buffer;
    src.connect(gain).connect(c.destination);
    src.start();
    return true;
  } catch (e) {
    console.warn('No se pudo reproducir el sonido', id, e);
    // The <audio> fallback needs a URL, which only the bundled sounds have.
    if (id === CUSTOM_SOUND) return false;
    const audio = new Audio(`/sounds/${id}.wav`);
    audio.volume = VOLUME;
    return audio.play().then(() => true, () => false);
  }
}
