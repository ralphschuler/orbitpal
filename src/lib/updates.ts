export interface UpdateCandidate {
  id: string;
  version: string;
  currentVersion: string;
  notes: string | null;
  date: string | null;
  releaseUrl: string;
}
export interface UpdateCheck {
  currentVersion: string;
  update: UpdateCandidate | null;
  installSupported: boolean;
  installUnavailableReason: string | null;
}
export interface UpdateProgress {
  candidateId: string;
  phase: 'downloading' | 'installing' | 'restarting' | 'error';
  downloaded: number;
  total?: number | null;
  message?: string | null;
}
export interface UpdateState {
  automaticChecks: boolean;
  settingsLoaded: boolean;
  savingSettings: boolean;
  checking: boolean;
  available: UpdateCandidate | null;
  installSupported: boolean;
  unavailableReason: string | null;
  phase: 'idle' | UpdateProgress['phase'];
  downloaded: number;
  total: number | null;
  lastChecked: number | null;
  error: string;
}
export const emptyUpdateState = (): UpdateState => ({
  automaticChecks: true, settingsLoaded: false, savingSettings: false,
  checking: false, available: null, installSupported: false, unavailableReason: null,
  phase: 'idle', downloaded: 0, total: null, lastChecked: null, error: '',
});
export const updateInProgress = (state: UpdateState) => ['downloading', 'installing', 'restarting'].includes(state.phase);

interface Dependencies {
  call: (command: string, args?: Record<string, unknown>) => Promise<any>;
  changed: (state: UpdateState) => void;
  lock: (installing: boolean) => void;
  errorText: (error: unknown) => string;
  now?: () => number;
}

/** Only checking is automatic. The only download path is an explicit confirm(). */
export class UpdateController {
  private state = emptyUpdateState();
  private alive = true;
  private confirmedId = '';
  constructor(private dependencies: Dependencies) {}
  private publish(patch: Partial<UpdateState>) {
    if (!this.alive) return;
    this.state = { ...this.state, ...patch };
    this.dependencies.changed({ ...this.state });
  }
  async load() {
    try {
      const settings = await this.dependencies.call('get_update_settings');
      this.publish({ automaticChecks: settings.automaticChecks, settingsLoaded: true });
    } catch (error) { this.publish({ error: this.dependencies.errorText(error) }); }
  }
  async automaticCheck() {
    if (this.alive && this.state.settingsLoaded && this.state.automaticChecks) await this.check();
  }
  async setAutomatic(automaticChecks: boolean) {
    if (!this.alive || this.state.savingSettings || updateInProgress(this.state)) return;
    this.publish({ savingSettings: true, error: '' });
    try {
      await this.dependencies.call('set_update_settings', { automaticChecks });
      this.publish({ automaticChecks });
    } catch (error) { this.publish({ error: this.dependencies.errorText(error) }); }
    finally { this.publish({ savingSettings: false }); }
  }
  async check() {
    if (!this.alive || this.state.checking || updateInProgress(this.state)) return;
    this.publish({ checking: true, available: null, error: '', phase: 'idle' });
    try {
      const result: UpdateCheck = await this.dependencies.call('check_update');
      this.publish({ available: result.update, installSupported: result.installSupported,
        unavailableReason: result.installUnavailableReason, lastChecked: (this.dependencies.now || Date.now)() });
    } catch (error) { this.publish({ error: this.dependencies.errorText(error) }); }
    finally { this.publish({ checking: false }); }
  }
  async confirm(candidateId: string, busyReason: string) {
    if (!this.alive || busyReason || this.state.checking || !this.state.installSupported || updateInProgress(this.state)
      || !this.state.available || this.state.available.id !== candidateId) return false;
    this.confirmedId = candidateId;
    this.dependencies.lock(true);
    this.publish({ phase: 'downloading', downloaded: 0, total: null, error: '' });
    try {
      await this.dependencies.call('install_update', { candidateId, confirmed: true });
      this.publish({ phase: 'restarting' });
      return true;
    } catch (error) {
      this.confirmedId = '';
      this.dependencies.lock(false);
      this.publish({ phase: 'error', available: null, error: this.dependencies.errorText(error) });
      return false;
    }
  }
  progress(event: UpdateProgress) {
    if (!this.alive || !this.confirmedId || event.candidateId !== this.confirmedId || !updateInProgress(this.state)) return;
    // Only the awaited install command releases the UI lock on failure. A late
    // event must not reopen the studio while native installation is still busy.
    if (event.phase === 'error') { this.publish({ error: event.message || 'Update fehlgeschlagen.' }); return; }
    const downloaded = Number.isFinite(event.downloaded) ? Math.max(this.state.downloaded, event.downloaded) : this.state.downloaded;
    const total = typeof event.total === 'number' && Number.isFinite(event.total) && event.total > 0 ? event.total : this.state.total;
    this.publish({ phase: event.phase, downloaded, total });
  }
  destroy() { this.alive = false; }
}
