// Thin typed wrapper around the Rust engine commands.
// When the page runs outside Tauri (plain `npm run dev` in a browser) a small
// fake engine stands in, so the UI can be worked on without audio hardware.

import { invoke } from "@tauri-apps/api/core";

export type Transport = "stopped" | "playing" | "recording";

export interface Status {
  transport: Transport;
  position_secs: number;
  length_secs: number;
  input_peak: number;
  output_peak: number;
  input_device: string | null;
  output_device: string | null;
  sample_rate: number;
  error: string | null;
}

export interface DeviceInfo {
  id: string;
  name: string;
  is_default: boolean;
}

export interface DeviceList {
  inputs: DeviceInfo[];
  outputs: DeviceInfo[];
}

export interface Engine {
  status(): Promise<Status>;
  peaks(bins: number): Promise<number[]>;
  record(): Promise<void>;
  play(): Promise<void>;
  stop(): Promise<void>;
  seek(secs: number): Promise<void>;
  listDevices(): Promise<DeviceList>;
  setDevices(input: string | null, output: string | null): Promise<void>;
  exportWav(path: string): Promise<void>;
  readonly isNative: boolean;
}

const native: Engine = {
  isNative: true,
  status: () => invoke("status"),
  peaks: (bins) => invoke("peaks", { bins }),
  record: () => invoke("record"),
  play: () => invoke("play"),
  stop: () => invoke("stop"),
  seek: (secs) => invoke("seek", { secs }),
  listDevices: () => invoke("list_devices"),
  setDevices: (input, output) => invoke("set_devices", { input, output }),
  exportWav: (path) => invoke("export_wav", { path }),
};

function fakeEngine(): Engine {
  const rate = 100; // "samples" per second, plenty for a fake waveform
  let samples: number[] = [];
  let transport: Transport = "stopped";
  let pos = 0;
  let last = performance.now();

  const tick = () => {
    const now = performance.now();
    const dt = (now - last) / 1000;
    last = now;
    if (transport === "recording") {
      const n = Math.round(dt * rate);
      for (let i = 0; i < n; i++) {
        const t = samples.length / rate;
        samples.push(Math.abs(Math.sin(t * 3)) * (0.3 + 0.5 * Math.random()));
      }
    } else if (transport === "playing") {
      pos += dt * rate;
      if (pos >= samples.length) {
        pos = 0;
        transport = "stopped";
      }
    }
  };

  return {
    isNative: false,
    async status() {
      tick();
      const level = transport === "recording" ? 0.3 + Math.random() * 0.5 : 0.02;
      return {
        transport,
        position_secs: (transport === "recording" ? samples.length : pos) / rate,
        length_secs: samples.length / rate,
        input_peak: level,
        output_peak: transport === "playing" ? (samples[Math.floor(pos)] ?? 0) : 0,
        input_device: "Browser preview (no audio)",
        output_device: "Browser preview (no audio)",
        sample_rate: 48000,
        error: null,
      };
    },
    async peaks(bins) {
      if (!samples.length) return [];
      return Array.from({ length: bins }, (_, b) => {
        const start = Math.floor((b * samples.length) / bins);
        const end = Math.max(start + 1, Math.floor(((b + 1) * samples.length) / bins));
        return Math.max(...samples.slice(start, end));
      });
    },
    async record() {
      tick();
      samples = [];
      pos = 0;
      transport = "recording";
    },
    async play() {
      tick();
      if (!samples.length) throw "Nothing recorded yet.";
      if (pos >= samples.length) pos = 0;
      transport = "playing";
    },
    async stop() {
      tick();
      if (transport === "stopped") pos = 0;
      transport = "stopped";
    },
    async seek(secs) {
      if (transport !== "recording") pos = Math.min(secs * rate, samples.length);
    },
    async listDevices() {
      const d = [{ id: "fake", name: "Browser preview (no audio)", is_default: true }];
      return { inputs: d, outputs: d };
    },
    async setDevices() {},
    async exportWav() {
      throw "Export only works in the desktop app.";
    },
  };
}

const inTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export const engine: Engine = inTauri ? native : fakeEngine();
