import { engine, type DeviceInfo, type Status } from "./engine";

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;

const btnRecord = $<HTMLButtonElement>("btn-record");
const btnPlay = $<HTMLButtonElement>("btn-play");
const btnStop = $<HTMLButtonElement>("btn-stop");
const btnExport = $<HTMLButtonElement>("btn-export");
const btnSettings = $<HTMLButtonElement>("btn-settings");
const clock = $("clock");
const meterIn = $("meter-in");
const inputName = $("input-name");
const devIn = $("dev-in");
const devOut = $("dev-out");
const lane = $("lane");
const canvas = $<HTMLCanvasElement>("wave");
const emptyHint = $("empty-hint");
const statusDevices = $("status-devices");
const statusMsg = $("status-msg");
const settings = $<HTMLDialogElement>("settings");
const selInput = $<HTMLSelectElement>("sel-input");
const selOutput = $<HTMLSelectElement>("sel-output");

let status: Status | null = null;
let peaks: number[] = [];
let peaksForLength = -1;
let lastPeaksFetch = 0;
let meterLevel = 0;
let msgTimer: number | undefined;

// ---------- helpers ----------

function formatTime(secs: number): string {
  const tenths = Math.floor(secs * 10);
  const m = Math.floor(tenths / 600);
  const s = (tenths % 600) / 10;
  return `${m}:${s.toFixed(1).padStart(4, "0")}`;
}

function showMessage(text: string, isError = false) {
  statusMsg.textContent = text;
  statusMsg.classList.toggle("error", isError);
  window.clearTimeout(msgTimer);
  msgTimer = window.setTimeout(() => (statusMsg.textContent = ""), 5000);
}

async function run(action: () => Promise<unknown>) {
  try {
    await action();
  } catch (e) {
    showMessage(String(e), true);
  }
}

function cssVar(name: string): string {
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim();
}

// ---------- drawing ----------

function draw() {
  const dpr = window.devicePixelRatio || 1;
  const w = lane.clientWidth;
  const h = lane.clientHeight;
  if (canvas.width !== Math.round(w * dpr) || canvas.height !== Math.round(h * dpr)) {
    canvas.width = Math.round(w * dpr);
    canvas.height = Math.round(h * dpr);
  }
  const ctx = canvas.getContext("2d")!;
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  ctx.clearRect(0, 0, w, h);

  const mid = h / 2;
  ctx.strokeStyle = cssVar("--line");
  ctx.beginPath();
  ctx.moveTo(0, mid);
  ctx.lineTo(w, mid);
  ctx.stroke();

  const recording = status?.transport === "recording";
  if (peaks.length) {
    ctx.fillStyle = recording ? cssVar("--record") : cssVar("--wave");
    const barW = w / peaks.length;
    for (let i = 0; i < peaks.length; i++) {
      // Lift quiet parts a little so a soft hum is still visible.
      const v = Math.min(1, Math.sqrt(peaks[i]));
      const bh = Math.max(1, v * (h - 16));
      ctx.fillRect(i * barW, mid - bh / 2, Math.max(1, barW - 0.5), bh);
    }
  }

  if (status && status.length_secs > 0 && !recording) {
    const x = (status.position_secs / status.length_secs) * w;
    ctx.fillStyle = cssVar("--text");
    ctx.fillRect(Math.round(x), 0, 2, h);
  }
}

// ---------- state → UI ----------

async function refreshPeaks(force = false) {
  if (!status) return;
  const now = performance.now();
  const recording = status.transport === "recording";
  const changed = status.length_secs !== peaksForLength;
  // While recording, redraw the growing waveform a few times a second.
  if (!force && (!changed || (recording && now - lastPeaksFetch < 150))) return;
  lastPeaksFetch = now;
  peaksForLength = status.length_secs;
  const bins = Math.max(1, Math.floor(lane.clientWidth / 3));
  try {
    peaks = await engine.peaks(bins);
  } catch {
    peaks = [];
  }
}

function render() {
  const s = status;
  const transport = s?.transport ?? "stopped";
  btnRecord.classList.toggle("active", transport === "recording");
  btnPlay.classList.toggle("active", transport === "playing");
  clock.textContent = formatTime(s?.position_secs ?? 0);
  btnExport.disabled = !s || s.length_secs === 0 || transport === "recording";
  emptyHint.hidden = !!s && (s.length_secs > 0 || transport === "recording");

  // Meter: jump up instantly, fall back smoothly.
  const peak = s?.input_peak ?? 0;
  meterLevel = Math.max(peak, meterLevel * 0.85);
  meterIn.style.width = `${Math.min(100, meterLevel * 100)}%`;
  meterIn.classList.toggle("hot", meterLevel > 0.7 && meterLevel < 0.98);
  meterIn.classList.toggle("clip", meterLevel >= 0.98);

  inputName.textContent = s?.input_device ?? "No input";
  devIn.textContent = s?.input_device ?? "No input";
  devOut.textContent = s?.output_device ?? "No output";
  if (s) {
    const rate = s.sample_rate ? ` · ${(s.sample_rate / 1000).toFixed(1)} kHz` : "";
    statusDevices.textContent = `Out: ${s.output_device ?? "none"}${rate}`;
  }
  draw();
}

let lastError: string | null = null;

async function poll() {
  try {
    status = await engine.status();
    if (status.error && status.error !== lastError) showMessage(status.error, true);
    lastError = status.error;
  } catch (e) {
    status = null;
    statusDevices.textContent = String(e);
  }
  await refreshPeaks();
  render();
  // Reopen the devices picked last time. The engine starts on the system
// defaults, and falls back to them if a remembered device is gone.
if (deviceChoice.input || deviceChoice.output) {
  run(() => engine.setDevices(deviceChoice.input, deviceChoice.output));
}

requestAnimationFrame(poll);
}

// ---------- actions ----------

const record = () =>
  run(async () => {
    if (status?.transport === "recording") {
      await engine.stop();
    } else {
      await engine.record();
      peaks = [];
    }
  });
const play = () => run(() => engine.play());
const stop = () => run(() => engine.stop());
const togglePlay = () => (status?.transport === "stopped" || !status ? play() : stop());

btnRecord.addEventListener("click", record);
btnPlay.addEventListener("click", play);
btnStop.addEventListener("click", stop);

lane.addEventListener("pointerdown", (e) => {
  if (!status || status.length_secs === 0) return;
  const rect = lane.getBoundingClientRect();
  const frac = Math.min(1, Math.max(0, (e.clientX - rect.left) / rect.width));
  run(() => engine.seek(frac * status!.length_secs));
});

window.addEventListener("keydown", (e) => {
  if (settings.open || e.repeat) return;
  if (e.code === "Space") {
    e.preventDefault();
    togglePlay();
  } else if (e.code === "KeyR") {
    e.preventDefault();
    record();
  } else if (e.code === "Home" || e.code === "Enter") {
    e.preventDefault();
    run(() => engine.seek(0));
  }
});

// Buttons keep focus after a click; stop Space from also "clicking" them.
document.querySelectorAll("button").forEach((b) =>
  b.addEventListener("keydown", (e) => {
    if (e.code === "Space" && !settings.open) e.preventDefault();
  }),
);

btnExport.addEventListener("click", () =>
  run(async () => {
    if (!engine.isNative) throw "Export only works in the desktop app.";
    const { save } = await import("@tauri-apps/plugin-dialog");
    const stamp = new Date().toISOString().slice(0, 16).replace(/[T:]/g, "-");
    const path = await save({
      defaultPath: `yoin-idea-${stamp}.wav`,
      filters: [{ name: "WAV audio", extensions: ["wav"] }],
    });
    if (!path) return;
    await engine.exportWav(path);
    showMessage(`Saved ${path}`);
  }),
);

// ---------- device choice (remembered between launches) ----------

const DEVICE_KEY = "yoin.devices";

interface DeviceChoice {
  input: string | null;
  output: string | null;
}

function loadDeviceChoice(): DeviceChoice {
  try {
    const raw = localStorage.getItem(DEVICE_KEY);
    if (raw) return JSON.parse(raw) as DeviceChoice;
  } catch {
    // Storage unavailable or corrupt: fall back to system defaults.
  }
  return { input: null, output: null };
}

function saveDeviceChoice(choice: DeviceChoice) {
  try {
    localStorage.setItem(DEVICE_KEY, JSON.stringify(choice));
  } catch {
    // Not fatal; the choice just won't survive a restart.
  }
}

let deviceChoice = loadDeviceChoice();

function fillSelect(sel: HTMLSelectElement, devices: DeviceInfo[], current: string | null) {
  sel.replaceChildren();
  sel.add(new Option("System default", ""));
  for (const d of devices) {
    sel.add(new Option(d.name + (d.is_default ? " (system default)" : ""), d.id));
  }
  // A remembered device that is unplugged now shows as "System default".
  sel.value = current && devices.some((d) => d.id === current) ? current : "";
}

btnSettings.addEventListener("click", () =>
  run(async () => {
    const list = await engine.listDevices();
    fillSelect(selInput, list.inputs, deviceChoice.input);
    fillSelect(selOutput, list.outputs, deviceChoice.output);
    settings.showModal();
  }),
);

settings.addEventListener("close", () => {
  if (settings.returnValue !== "apply") return;
  run(async () => {
    const choice = { input: selInput.value || null, output: selOutput.value || null };
    await engine.setDevices(choice.input, choice.output);
    deviceChoice = choice;
    saveDeviceChoice(choice);
    peaksForLength = -1;
    showMessage("Audio devices updated");
  });
});

window.addEventListener("resize", () => {
  peaksForLength = -1;
});

// Reopen the devices picked last time. The engine starts on the system
// defaults, and falls back to them if a remembered device is gone.
if (deviceChoice.input || deviceChoice.output) {
  run(() => engine.setDevices(deviceChoice.input, deviceChoice.output));
}

requestAnimationFrame(poll);
