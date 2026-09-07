import { Channel, invoke } from "@tauri-apps/api/core";
import type { StreamEvent } from "../types";

/** Akışlı bir decompile/decode komutunu çalıştırır; olayları callback'e verir. */
export async function runStream(
  command: string,
  args: Record<string, unknown>,
  onEvent: (ev: StreamEvent) => void
): Promise<void> {
  const channel = new Channel<StreamEvent>();
  channel.onmessage = onEvent;
  await invoke(command, { ...args, onEvent: channel });
}

export interface StreamState {
  active: boolean;
  percent: number; // -1 = belirsiz (indeterminate)
  current: number;
  total: number;
  lastLine: string;
}

export const IDLE_STREAM: StreamState = {
  active: false,
  percent: -1,
  current: 0,
  total: 0,
  lastLine: "",
};

/** Canlı ilerleme çubuğu — belirsiz modda (percent<0) kayan animasyon gösterir. */
export function ProgressBar({ state }: { state: StreamState }) {
  if (!state.active && !state.lastLine) return null;
  const indeterminate = state.percent < 0;
  return (
    <div className="stream-progress">
      <div className="stream-bar-track">
        <div
          className={`stream-bar-fill ${indeterminate ? "indet" : ""}`}
          style={indeterminate ? undefined : { width: `${state.percent}%` }}
        />
      </div>
      <div className="stream-meta">
        <span className="stream-pct mono">
          {indeterminate ? (state.active ? "işleniyor…" : "") : `%${state.percent}`}
          {state.total > 0 && ` · ${state.current.toLocaleString()}/${state.total.toLocaleString()}`}
        </span>
        <span className="stream-line mono muted" title={state.lastLine}>
          {state.lastLine}
        </span>
      </div>
    </div>
  );
}
