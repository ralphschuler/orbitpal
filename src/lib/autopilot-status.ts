import type { AutopilotSchedule } from './autopilot';

// These app-owned reasons are also the real scheduling guard, so the status
// cannot drift away from a separate copy of the blocking expression.
export const speechBlockerMessages = {
  permission: 'Wartet: Berechtigungsabfrage abschließen.',
  update: 'Wartet: App-Update abschließen.',
  starting: 'Wartet: Autopilot wird gestartet.',
  reply: 'Wartet: Eine Antwort oder Sprachausgabe läuft.',
  sound: 'Wartet: Ein Emote-Sound läuft.',
  recording: 'Wartet: Mikrofonaufnahme beenden.',
  transcription: 'Wartet: Spracherkennung abschließen.',
  saving: 'Wartet: Einstellungen werden gespeichert.',
  botChange: 'Wartet: Botwechsel oder Neuerstellung abschließen.',
  extensions: 'Wartet: Erweiterungseditor schließen.',
  botDialog: 'Wartet: Bot-Dialog schließen.',
  updatesDialog: 'Wartet: „Updates & Version“ schließen.',
  unsaved: 'Wartet: Änderungen speichern.',
  draft: 'Wartet: Nachrichtenentwurf senden oder leeren.',
  secretDraft: 'Wartet: Zugangsdaten speichern oder den Entwurf leeren.',
} as const;
export type SpeechBlocker = keyof typeof speechBlockerMessages;
export function speechBlocker(activity: Partial<Record<SpeechBlocker, boolean>>): SpeechBlocker | undefined {
  return (Object.keys(speechBlockerMessages) as SpeechBlocker[]).find(reason => activity[reason] === true);
}

export interface SpeechStatusInput {
  active: boolean;
  speak: boolean;
  outputAvailable: boolean;
  modelConfigured: boolean;
  blocked?: SpeechBlocker;
  schedule?: AutopilotSchedule;
}
export interface SpeechStatus {
  message: string;
  parameters?: { time: string };
  waiting: boolean;
}
export const speechStatusMessages = {
  inactive: 'Autopilot pausiert.',
  disabled: 'Selbstständiges Sprechen ist ausgeschaltet.',
  model: 'Wartet: Dialogmodell auswählen und speichern.',
  output: 'Wartet: Studioansicht oder eine aktive Figurenausgabe auswählen.',
  working: 'Eigener Beitrag wird vorbereitet oder gesprochen.',
  otherBot: 'Wartet: Eine andere Figur bereitet ihren Beitrag vor oder spricht.',
  initializing: 'Sprechzeitplan wird vorbereitet.',
  budget: 'Stundenlimit erreicht. Nächster Sprechversuch frühestens in {time}.',
  countdown: 'Nächster Sprechversuch in etwa {time}.',
  ready: 'Bereit für den nächsten Sprechversuch.',
} as const;

export function countdown(seconds: number): string {
  const safe = Number.isFinite(seconds) ? Math.max(0, Math.ceil(seconds)) : 0;
  return `${Math.floor(safe / 60)}:${String(safe % 60).padStart(2, '0')}`;
}

export function speechStatus(input: SpeechStatusInput): SpeechStatus {
  const status = (key: keyof typeof speechStatusMessages, waiting = false): SpeechStatus => ({ message: speechStatusMessages[key], waiting });
  if (!input.active) return status('inactive');
  if (!input.speak) return status('disabled', true);
  if (input.schedule?.inFlight) return status('working');
  if (!input.modelConfigured) return status('model', true);
  if (!input.outputAvailable) return status('output', true);
  if (input.blocked) return { message: speechBlockerMessages[input.blocked], waiting: true };
  if (input.schedule?.waitingForBot) return status('otherBot', true);
  if (!input.schedule) return status('initializing');
  if (!input.schedule.eligible) return status('initializing', true);
  if (input.schedule.budgetLimited) return { message: speechStatusMessages.budget, parameters: { time: countdown(input.schedule.remainingSeconds) }, waiting: true };
  if (input.schedule.remainingSeconds > 0) return { message: speechStatusMessages.countdown, parameters: { time: countdown(input.schedule.remainingSeconds) }, waiting: false };
  return status('ready');
}
