// Editor benchmark statistics over `viewport.stats` frame records.

/** A native-viewport frame, as `viewport.stats` reports it (times in Unix ms). */
export interface FrameRecord {
  seq: number | null;
  input_time_ms: number | null;
  cpu_ms: number;
  submitted_ms: number;
  presented_ms: number | null;
}

/** Percentiles of one measure (ms). */
export interface Distribution {
  p50: number;
  p95: number;
  p99: number;
  max: number;
}

/** A scenario's frame summary. */
export interface FrameSummary {
  frames: number;
  unpresented: number;
  fps: number;
  /** Intervals between consecutive presented frames. */
  interval_ms: Distribution;
  /** Display refreshes with no new frame. */
  missed_refreshes: number;
  /** Input to the first presented frame showing it. */
  latency_ms: Distribution;
  /** Inputs that reached a presented frame. */
  inputs: number;
  cpu_ms: Distribution;
}

/** Nearest-rank percentile; NaN for no values. */
export function percentile(values: readonly number[], p: number): number {
  if (!(p >= 0 && p <= 100)) throw new RangeError(`percentile ${p} is outside 0..100`);
  if (values.length === 0) return Number.NaN;
  const sorted = [...values].sort((a, b) => a - b);
  const rank = Math.max(1, Math.ceil((p / 100) * sorted.length));
  return sorted[rank - 1] ?? Number.NaN;
}

function distribution(values: readonly number[]): Distribution {
  return {
    p50: percentile(values, 50),
    p95: percentile(values, 95),
    p99: percentile(values, 99),
    max: values.length === 0 ? Number.NaN : Math.max(...values),
  };
}

/** First presentation time per input sequence number. */
function firstPresented(frames: readonly FrameRecord[]): Map<number, { input: number; presented: number }> {
  const first = new Map<number, { input: number; presented: number }>();
  for (const f of frames) {
    if (f.seq === null || f.input_time_ms === null || f.presented_ms === null) continue;
    const seen = first.get(f.seq);
    if (seen === undefined || f.presented_ms < seen.presented)
      first.set(f.seq, { input: f.input_time_ms, presented: f.presented_ms });
  }
  return first;
}

/** Summarizes frames for a display refreshing every `refreshMs`. */
export function summarize(frames: readonly FrameRecord[], refreshMs: number): FrameSummary {
  if (!(refreshMs > 0)) throw new RangeError(`refresh interval ${refreshMs} must be positive`);
  const presented = frames.flatMap((f) => (f.presented_ms === null ? [] : [f.presented_ms])).sort((a, b) => a - b);
  const intervals = presented.slice(1).map((t, i) => t - (presented[i] ?? t));
  const span = (presented.at(-1) ?? 0) - (presented[0] ?? 0);
  const latencies = [...firstPresented(frames).values()].map((v) => v.presented - v.input);
  return {
    frames: presented.length,
    unpresented: frames.length - presented.length,
    fps: span > 0 ? (intervals.length / span) * 1000 : 0,
    interval_ms: distribution(intervals),
    missed_refreshes: intervals.reduce((n, i) => n + Math.max(0, Math.round(i / refreshMs) - 1), 0),
    latency_ms: distribution(latencies),
    inputs: latencies.length,
    cpu_ms: distribution(frames.map((f) => f.cpu_ms)),
  };
}
