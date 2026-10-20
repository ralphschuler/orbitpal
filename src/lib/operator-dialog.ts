import { open as openNative, save as saveNative, type OpenDialogOptions, type SaveDialogOptions } from '@tauri-apps/plugin-dialog';
import { writable } from 'svelte/store';
import { native } from './api';

export const operatorDialogActive = writable(false);
let dialogs = 0;
let registration: { identity: symbol; block: () => Promise<void> } | undefined;
export function registerOperatorDialogBarrier(block: () => Promise<void>): () => void {
  const identity = Symbol('Studio dialog barrier');
  registration = { identity, block };
  return () => { if (registration?.identity === identity) registration = undefined; };
}
async function dialog<T>(operation: () => Promise<T>): Promise<T> {
  dialogs++; operatorDialogActive.set(true);
  try {
    if (native) {
      const barrier = registration;
      if (!barrier) throw new Error('Die Studio-Verbindung ist noch nicht bereit. Vor dem Dateidialog erneut verbinden.');
      await barrier.block();
      if (registration !== barrier) throw new Error('Der Dateidialog wurde durch einen Studio-Neustart abgebrochen.');
    }
    return await operation();
  } finally { dialogs--; operatorDialogActive.set(dialogs > 0); }
}
export function open(options?: OpenDialogOptions): Promise<string | string[] | null> { return dialog(() => openNative(options)); }
export function save(options?: SaveDialogOptions): Promise<string | null> { return dialog(() => saveNative(options)); }
