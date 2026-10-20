type Invoke = (command: string, args?: Record<string, unknown>) => Promise<unknown>;
export interface LifeUIProof { connectionId: string; sequence: number; }
const failure = () => new Error('Die Studio-Verbindung zur nativen Laufzeit ist nicht bestätigt. Stop bleibt verfügbar. Erneut verbinden.');
const uuid = (value: unknown): value is string => typeof value === 'string' && /^[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$/i.test(value);
function deadline<T>(work: Promise<T>, milliseconds: number): Promise<T> {
  let timer: ReturnType<typeof setTimeout>;
  return Promise.race([work, new Promise<never>((_, reject) => { timer = setTimeout(() => reject(failure()), milliseconds); })]).finally(() => clearTimeout(timer));
}

/** Session-only status reporter. Serial ordering and native-issued connection
 * identity prevent an old clear/disconnect from superseding a newer blocker.
 * No report starts a bot or claims that the OS/window is hidden or unlocked. */
export class LifeUI {
  private connection?: string;
  private epoch = 0;
  private sequence = 0;
  private disposed = false;
  private queue: Promise<unknown> = Promise.resolve();
  private pending = 0;
  constructor(private invoke: Invoke, private changed: (available: boolean) => void, private milliseconds = 2000) {}
  private disconnect(id: string) { void this.invoke('disconnect_life_ui', { connectionId: id }).catch(() => {}); }
  async connect(): Promise<void> {
    if (this.disposed) throw failure();
    const epoch = ++this.epoch, previous = this.connection;
    this.connection = undefined; this.sequence = 0; this.changed(false);
    if (previous) this.disconnect(previous);
    const pending = this.invoke('connect_life_ui', {});
    // A timed-out/disposed connection still owns its late returned identity.
    // Dispose that identity only; a newer connection must never be cleared.
    void pending.then(value => {
      const id = (value as { connectionId?: unknown } | null)?.connectionId;
      if ((this.disposed || epoch !== this.epoch) && uuid(id)) this.disconnect(id);
    }).catch(() => {});
    let value: unknown;
    try { value = await deadline(pending, this.milliseconds); }
    catch { if (epoch === this.epoch) { ++this.epoch; this.changed(false); } throw failure(); }
    const id = (value as { connectionId?: unknown } | null)?.connectionId;
    if (this.disposed || epoch !== this.epoch || !uuid(id)) throw failure();
    this.connection = id;
    try { await this.report(true, false); }
    catch { throw failure(); }
  }
  report(blocked: boolean, activity: boolean): Promise<LifeUIProof> {
    return this.enqueueReport(blocked, activity);
  }
  /** Dispatch an explicit operator command immediately after its actual report
   * acknowledgement, before a queued heartbeat can supersede that proof. The
   * command promise never holds this queue: newer real blockers remain prompt. */
  reportProof<T>(blocked: boolean, activity: boolean, dispatch: (proof: LifeUIProof) => Promise<T>): Promise<T> {
    let result: Promise<T> | undefined;
    return this.enqueueReport(blocked, activity, proof => {
      try { result = Promise.resolve(dispatch(proof)); }
      catch (error) { result = Promise.reject(error); }
      // Attach a rejection handler now; the outer report is still unwinding.
      void result.catch(() => {});
    }).then(() => {
      if (!result) throw failure();
      return result;
    });
  }
  private enqueueReport(blocked: boolean, activity: boolean, afterAcknowledged?: (proof: LifeUIProof) => void): Promise<LifeUIProof> {
    const connection = this.connection, epoch = this.epoch;
    if (this.disposed || !connection) return Promise.reject(failure());
    if (this.pending >= 32) {
      this.connection = undefined; ++this.epoch; this.changed(false); this.disconnect(connection);
      return Promise.reject(failure());
    }
    this.pending++;
    const work = this.queue.catch(() => {}).then(async () => {
      if (this.disposed || epoch !== this.epoch || connection !== this.connection) throw failure();
      if (this.sequence >= Number.MAX_SAFE_INTEGER) throw failure();
      const sequence = ++this.sequence;
      const acknowledged = await deadline(this.invoke('report_life_ui', { report: { connectionId: connection, sequence, blocked, activity } }), this.milliseconds);
      if (this.disposed || epoch !== this.epoch || connection !== this.connection || acknowledged !== true) throw failure();
      this.changed(true);
      const proof = { connectionId: connection, sequence };
      afterAcknowledged?.(proof);
      return proof;
    });
    this.queue = work;
    return work.catch(() => {
      if (epoch === this.epoch && connection === this.connection) {
        this.connection = undefined; ++this.epoch; this.changed(false); this.disconnect(connection);
      }
      throw failure();
    }).finally(() => { this.pending--; });
  }
  dispose(): void {
    this.disposed = true; ++this.epoch;
    const previous = this.connection; this.connection = undefined; this.changed(false);
    if (previous) this.disconnect(previous);
  }
}
