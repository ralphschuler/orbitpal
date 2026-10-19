import { writable } from 'svelte/store';
import { call, native } from './api';

export type SystemPermissionKind = 'screen' | 'microphone';
export interface SystemPermission { platform: string; status: string }
export interface SystemPermissionDialog {
  kind: SystemPermissionKind;
  permission: SystemPermission | null;
  loading: boolean;
  error: unknown;
  recovery: boolean;
  visible: boolean;
}
const commands = {
  screen: { get: 'get_screen_capture_permission', request: 'request_screen_capture_permission', settings: 'open_screen_capture_settings' },
  microphone: { get: 'get_microphone_permission', request: 'request_microphone_permission', settings: 'open_microphone_settings' },
} as const;
const statuses = {
  screen: ['authorized', 'notGranted', 'systemManaged', 'unsupported'],
  microphone: ['authorized', 'notDetermined', 'denied', 'restricted', 'webview', 'unknown'],
};
export function usablePermission(kind: SystemPermissionKind, permission: SystemPermission | null): boolean {
  return !!permission && (permission.status === 'authorized' || kind === 'screen' && permission.status === 'systemManaged' || kind === 'microphone' && permission.status === 'webview');
}
function validPermission(kind: SystemPermissionKind, value: unknown): SystemPermission {
  if (!value || typeof value !== 'object' || typeof (value as SystemPermission).platform !== 'string' || !statuses[kind].includes((value as SystemPermission).status)) {
    throw new Error('Die App hat keinen gültigen Berechtigungsstatus geliefert.');
  }
  return value as SystemPermission;
}

/** A suspended operator intent, never an authorization for a bot. Refreshing a
 * status cannot resume an action. Only Request/Continue can resolve it true. */
export class SystemPermissionFlow {
  private store = writable<SystemPermissionDialog | null>(null);
  readonly subscribe = this.store.subscribe;
  private pending: { id: number; current: () => boolean; resolve: (granted: boolean) => void; state: SystemPermissionDialog } | null = null;
  private revision = 0;
  constructor(private invoke: (command: string) => Promise<unknown>, private isNative: () => boolean) {}
  get active() { return !!this.pending; }
  private publish() { this.store.set(this.pending?.state || null); }
  cancel() {
    const pending = this.pending; this.pending = null; ++this.revision;
    this.publish(); pending?.resolve(false);
  }
  invalidate() { if (this.pending && !this.pending.current()) this.cancel(); }
  private current(id: number) {
    this.invalidate();
    return !!this.pending && this.pending.id === id;
  }
  async inspect(kind: SystemPermissionKind): Promise<SystemPermission> {
    if (!this.isNative()) return { platform: 'browser', status: kind === 'microphone' ? 'webview' : 'unsupported' };
    return validPermission(kind, await this.invoke(commands[kind].get));
  }
  async ensure(kind: SystemPermissionKind, current: () => boolean = () => true): Promise<boolean> {
    if (!current() || this.pending) return false;
    return new Promise(resolve => {
      const id = ++this.revision;
      this.pending = { id, current, resolve, state: { kind, permission: null, loading: true, error: null, recovery: false, visible: false } };
      this.publish();
      void this.check(id, false, true);
    });
  }
  /** Background failures show a recovery path, with no retained request to replay. */
  recover(kind: SystemPermissionKind, error: unknown = null) {
    if (this.pending) return;
    const id = ++this.revision;
    this.pending = { id, current: () => true, resolve: () => {}, state: { kind, permission: null, loading: true, error, recovery: true, visible: true } };
    this.publish(); void this.check(id, false, false, true);
  }
  async refresh() { if (this.pending && !this.pending.state.loading) await this.check(this.pending.id, false, false); }
  async request() { if (this.pending && !this.pending.state.loading) await this.check(this.pending.id, true, false); }
  async settings() {
    const pending = this.pending;
    if (!pending || pending.state.loading || !this.isNative()) return;
    try { await this.invoke(commands[pending.state.kind].settings); }
    catch (error) { if (this.current(pending.id)) { pending.state = { ...pending.state, error }; this.publish(); } }
  }
  async continue() {
    // A previous check can become stale while Settings is open. Recheck from the
    // explicit Continue click before resolving, and never prompt from here.
    if (this.pending && !this.pending.state.loading) await this.check(this.pending.id, false, true);
  }
  private async check(id: number, request: boolean, continueGranted: boolean, preserveError = false) {
    if (!this.current(id)) return;
    const pending = this.pending!;
    pending.state = { ...pending.state, loading: true, error: preserveError ? pending.state.error : null }; this.publish();
    try {
      const permission = request && this.isNative()
        ? validPermission(pending.state.kind, await this.invoke(commands[pending.state.kind].request))
        : await this.inspect(pending.state.kind);
      if (!this.current(id)) return;
      pending.state = { ...pending.state, permission };
      if (!pending.state.recovery && usablePermission(pending.state.kind, permission) && (continueGranted || request)) {
        this.pending = null; this.publish(); pending.resolve(true); return;
      }
    } catch (error) {
      if (!this.current(id)) return;
      pending.state = { ...pending.state, permission: null, error };
    }
    if (this.current(id)) { pending.state = { ...pending.state, loading: false, visible: true }; this.publish(); }
  }
}
export const systemPermission = new SystemPermissionFlow(command => call(command), () => native);
