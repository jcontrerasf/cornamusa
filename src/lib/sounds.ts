// New-mail sounds bundled with the app (synthesized by assets/sounds/generate.py).

export const SOUNDS = [
  { id: 'corneta-suave', label: 'Corneta suave' },
  { id: 'corneta-cartero', label: 'Corneta de cartero' },
  { id: 'gaita-breve', label: 'Gaita breve' },
] as const;

const VOLUME = 0.8;

export function isBundledSound(id: string): boolean {
  return SOUNDS.some((s) => s.id === id);
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
    p = fetch(`/sounds/${id}.wav`)
      .then((r) => {
        if (!r.ok) throw new Error(`HTTP ${r.status}`);
        return r.arrayBuffer();
      })
      .then((data) => c.decodeAudioData(data));
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

/** Plays a bundled sound; silently ignores unknown ids and playback errors. */
export async function playSound(id: string) {
  if (!isBundledSound(id)) return;
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
  } catch (e) {
    console.warn('No se pudo reproducir el sonido', id, e);
    const audio = new Audio(`/sounds/${id}.wav`);
    audio.volume = VOLUME;
    audio.play().catch(() => {});
  }
}
