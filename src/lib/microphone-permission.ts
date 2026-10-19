import { writable } from 'svelte/store';
import { call, native } from './api';

export type MicrophoneStatus = 'notDetermined' | 'restricted' | 'denied' | 'authorized' | 'webview' | 'unknown';
export interface MicrophonePermission { platform: string; status: MicrophoneStatus; }
export const microphoneFailure = writable('');
export class MicrophonePermissionError extends Error {}

export function microphonePermissionMessage(status: MicrophoneStatus): string {
  if (status === 'restricted') return 'macOS beschränkt den Mikrofonzugriff. Prüfe Bildschirmzeit oder die Geräteverwaltung. Anschließend musst du die Aufnahme selbst erneut starten.';
  if (status === 'denied') return 'macOS hat den Mikrofonzugriff für OrbitPal nicht freigegeben. Öffne Systemeinstellungen → Datenschutz & Sicherheit → Mikrofon, aktiviere die App und starte sie bei Bedarf neu. Klicke danach erneut auf Aufnahme oder Zuhören starten.';
  if (status === 'notDetermined') return 'macOS hat die Mikrofonfreigabe noch nicht beantwortet. Klicke erneut auf Aufnahme oder Zuhören starten und bestätige die Systemabfrage.';
  return 'Der Mikrofonstatus konnte nicht bestimmt werden. Prüfe die Mikrofonfreigabe in den Systemeinstellungen und starte die Aufnahme anschließend selbst erneut.';
}

/** Called only from the user's explicit recording/listening Start, never from
 * saved settings, retries, returning from System Settings, or a model tool. */
export async function ensureMicrophonePermission(stillCurrent: () => boolean = () => true): Promise<boolean> {
  if (!stillCurrent()) return false;
  microphoneFailure.set('');
  if (!native) return stillCurrent();
  let permission: MicrophonePermission;
  try { permission = await call<MicrophonePermission>('request_microphone_permission'); }
  catch (error) {
    if (!stillCurrent()) return false;
    const message = `Mikrofonfreigabe konnte nicht geprüft werden: ${error instanceof Error ? error.message : String(error)}`;
    microphoneFailure.set(message);
    throw new MicrophonePermissionError(message);
  }
  if (!stillCurrent()) return false;
  if (permission.status === 'authorized' || permission.status === 'webview') return true;
  const message = microphonePermissionMessage(permission.status);
  microphoneFailure.set(message);
  throw new MicrophonePermissionError(message);
}

export function reportMicrophoneFailure(error: unknown): string | undefined {
  const name = error instanceof Error ? error.name : '';
  if (['NotAllowedError', 'SecurityError'].includes(name)) {
    const message = 'Der Mikrofonzugriff wurde vom System oder von der WebView abgelehnt. Prüfe die Mikrofonfreigabe für OrbitPal in den Systemeinstellungen. Beende und öffne die App nach einer geänderten Freigabe erneut und starte die Aufnahme selbst. Nach einem Update kann die Freigabe erneut nötig sein.';
    microphoneFailure.set(message);
    return message;
  }
}

export async function openMicrophone(constraints: MediaStreamConstraints): Promise<MediaStream> {
  try { return await navigator.mediaDevices.getUserMedia(constraints); }
  catch (error) {
    const message = reportMicrophoneFailure(error);
    if (message) throw Object.assign(new Error(message), { name: (error as Error).name });
    throw error;
  }
}
