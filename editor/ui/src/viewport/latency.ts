// Input tags for input-to-photon measurement: each camera-changing input gets a sequence
// number and its Unix-ms time; `viewport.present` echoes both in `viewport.stats`.

/** One input's tag. */
export interface InputTag {
  seq: number;
  input_time_ms: number;
}

/** Numbers inputs and converts event timestamps to Unix ms. */
export class InputTags {
  private last: InputTag | undefined;

  /** `origin` is the page clock's Unix-ms origin (`performance.timeOrigin`). */
  constructor(private readonly origin: () => number = () => performance.timeOrigin) {}

  /** Tags an input whose event had `timeStamp` (ms since the page clock's origin). */
  mark(timeStamp: number): InputTag {
    if (!Number.isFinite(timeStamp)) throw new RangeError(`bad event timestamp ${timeStamp}`);
    this.last = { seq: (this.last?.seq ?? 0) + 1, input_time_ms: this.origin() + timeStamp };
    return this.last;
  }

  /** The newest tag, if any input happened. */
  current(): InputTag | undefined {
    return this.last;
  }
}

/** The editor's input tags (one page, one viewport). */
export const inputTags = new InputTags();
