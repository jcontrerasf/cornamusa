// New-mail sounds bundled with the app (synthesized by assets/sounds/generate.py).

export const SOUNDS = [
  { id: 'corneta-suave', label: 'Corneta suave' },
  { id: 'corneta-cartero', label: 'Corneta de cartero' },
  { id: 'gaita-breve', label: 'Gaita breve' },
] as const;

const cache = new Map<string, HTMLAudioElement>();

export function isBundledSound(id: string): boolean {
  return SOUNDS.some((s) => s.id === id);
}

/** Plays a bundled sound; silently ignores unknown ids and playback errors. */
export function playSound(id: string) {
  if (!isBundledSound(id)) return;
  let audio = cache.get(id);
  if (!audio) {
    audio = new Audio(`/sounds/${id}.wav`);
    audio.volume = 0.8;
    cache.set(id, audio);
  }
  audio.currentTime = 0;
  audio.play().catch(() => {});
}
