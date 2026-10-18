export type Shortcut = 'save' | 'send' | 'focus-message' | 'help' | 'stop' | 'dismiss' | { page: number };
export type ShortcutKey = Pick<KeyboardEvent, 'key' | 'code' | 'ctrlKey' | 'metaKey' | 'altKey' | 'shiftKey' | 'repeat' | 'isComposing'>;
export function shortcutFor(key: ShortcutKey, context: { editing: boolean; modal: boolean; studio: boolean }): Shortcut | null {
  if (key.isComposing || key.repeat) return null;
  const modifier = key.ctrlKey || key.metaKey;
  const letter = key.key.toLowerCase();
  if (key.key === 'Escape') return context.modal ? 'dismiss' : 'stop';
  if (context.modal) return modifier && !key.altKey && letter === 's' ? 'save' : null;
  if (modifier && !key.altKey && !key.shiftKey) {
    if (letter === 's') return 'save';
    if (key.key === 'Enter' && context.studio) return 'send';
    if (letter === 'k') return 'focus-message';
  }
  if (key.key === 'F1' || (!context.editing && !modifier && !key.altKey && key.key === '?')) return 'help';
  // Use physical digit codes: macOS Option changes event.key, and AltGr must
  // remain available for normal text input rather than switching settings.
  if (key.altKey && !modifier && !key.shiftKey && !context.editing) {
    const digit = /^Digit([1-8])$/.exec(key.code)?.[1];
    if (digit) return { page: Number(digit) - 1 };
  }
  return null;
}
