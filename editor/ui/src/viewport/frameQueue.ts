// At most one viewport frame in flight; the newest request waits and replaces older waiting ones.
// Each job carries everything it renders (view, size, world), so a job that runs later still draws
// what was asked for when it was requested.

export class FrameQueue<Job> {
  private busy = false;
  private next: Job | undefined;

  constructor(private readonly render: (job: Job) => Promise<unknown>) {}

  /** Asks for a frame; it runs now, or after the frame in flight. */
  request(job: Job): void {
    this.next = job;
    this.run();
  }

  private run(): void {
    const job = this.next;
    if (this.busy || job === undefined) return;
    this.busy = true;
    this.next = undefined;
    this.render(job)
      .catch(() => undefined)
      .finally(() => {
        this.busy = false;
        this.run();
      });
  }
}
