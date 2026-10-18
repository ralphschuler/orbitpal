/** Subtitles belong to a playback job, independently of the saved history.
 * Keep them throughout speech, then for 15 seconds after that job ends. */
export class SubtitleLifetime {
  private id?: string;
  private generation = 0;
  private timer?: ReturnType<typeof setTimeout>;
  private finished = false;
  private destroyed = false;

  constructor(private onText: (text: string) => void) {}

  start(id: string, text: string): void {
    if (this.destroyed) return;
    this.clearTimer();
    ++this.generation;
    this.id = id;
    this.finished = false;
    this.onText(text);
  }

  finish(id: string): void {
    if (this.destroyed || id !== this.id || this.finished) return;
    this.finished = true;
    const generation = this.generation;
    this.timer = setTimeout(() => {
      if (this.destroyed || generation !== this.generation || id !== this.id) return;
      this.timer = undefined;
      this.id = undefined;
      this.finished = false;
      this.onText('');
    }, 15_000);
  }

  cancel(id?: string): void {
    if (this.destroyed || (id !== undefined && id !== this.id)) return;
    this.clearTimer();
    ++this.generation;
    this.id = undefined;
    this.finished = false;
    this.onText('');
  }

  destroy(): void {
    if (this.destroyed) return;
    this.cancel();
    this.destroyed = true;
  }

  private clearTimer(): void {
    if (this.timer !== undefined) clearTimeout(this.timer);
    this.timer = undefined;
  }
}
