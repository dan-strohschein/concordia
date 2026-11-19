// Coalesced refreshes: while one run is in flight, any number of requests collapse into a single
// trailing run (a drag's stream of change events costs two fetches, not one per event).

/** Runs `run` on request, at most one at a time, with at most one queued behind it. */
export class Coalescer {
  private running = false;
  private again = false;

  constructor(private readonly run: () => Promise<unknown>) {}

  /** Runs now, or once more after the current run. */
  request(): void {
    if (this.running) {
      this.again = true;
      return;
    }
    this.running = true;
    this.run()
      .catch(() => undefined)
      .finally(() => {
        this.running = false;
        if (this.again) {
          this.again = false;
          this.request();
        }
      });
  }
}
