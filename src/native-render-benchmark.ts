import { invoke } from '@tauri-apps/api/core';
import type { AvatarStage } from './lib/avatar';
import { BenchmarkBatcher, FreshSubmissionGate, parseBenchmarkPage, StageDiagnosticCounters } from './lib/stage-diagnostics';

// This entry never imports main.ts/App/Overlay, profiles, Store or plugins.
// Native independently authenticates its unique figure label + issued epoch.
const page = parseBenchmarkPage(window.location.search);
const status = document.getElementById('status')!;
const viewport = document.getElementById('viewport')!;
const host = document.getElementById('figure')!;
const profile = {
  id: 'benchmark-nova',
  avatar: { assetPath: null, accent: '#80b8e8', scale: 1, camera: 'full', idle: true, autoBlink: true },
  output: { mode: 'studio', fps: 30 },
  voice: { enabled: false, volume: 0 },
  emotes: [], motions: [],
};

async function figure(epoch: number): Promise<void> {
  const diagnostics = new StageDiagnosticCounters();
  const fresh = new FreshSubmissionGate();
  let stage: AvatarStage | undefined;
  let pageClosed = false;
  let disposed = false;
  let domOut = false;
  const batcher = new BenchmarkBatcher(epoch, () => {
    const snapshot = diagnostics.snapshot();
    return { ...snapshot, telemetryInvalid: snapshot.telemetryInvalid || stage?.telemetryInvalid === true, disposed, domOut };
  }, packet => invoke('bench_batch', { packet }), command => {
    if (command === 'dispose') {
      if (disposed || !stage) throw new Error('Benchmark lifecycle unavailable');
      stage.dispose(); // Throws => no disposed/final packet, never optimistic ACK.
      disposed = true;
      return;
    }
    if (disposed) throw new Error('Benchmark already disposed');
    domOut = command === 'dom_out';
    // Fixed dimensions remain unchanged; this is DOM-out-of-view only.
    host.style.transform = domOut ? 'translateX(calc(100% + 1px))' : '';
  }, () => diagnostics.invalidate());
  window.addEventListener('error', event => { diagnostics.observe({ kind: 'failure' }); event.preventDefault(); });
  window.addEventListener('unhandledrejection', event => { diagnostics.observe({ kind: 'failure' }); event.preventDefault(); });
  window.addEventListener('pagehide', () => {
    pageClosed = true;
    batcher.stop();
    fresh.cancel();
    if (!disposed && stage) {
      try { stage.dispose(); disposed = true; } catch { diagnostics.invalidate(); }
    }
  }, { once: true });
  status.hidden = true;
  viewport.hidden = false;
  try {
    const { AvatarStage: ActualStage } = await import('./lib/avatar');
    if (pageClosed) return;
    stage = new ActualStage(host, { profile, motionSource: 'fixed', speaking: false, level: 0, emotion: 'neutral',
      transparent: false, audioEnabled: false }, () => diagnostics.observe({ kind: 'failure' }), undefined,
      observation => { diagnostics.observe(observation); fresh.observe(observation); });
    await stage.loadAsset(undefined, [], '', profile.id);
    if (pageClosed) return; // pagehide already disposed the retained exact Stage.
    await fresh.armAfterLoad();
  } catch { diagnostics.observe({ kind: 'failure' }); }
  // A failed constructor/load is still numeric failure telemetry. Native never
  // accepts it as a first-draw qualification or successful lifecycle.
  if (!pageClosed) batcher.start();
}

if (page?.kind === 'figure') void figure(page.epoch);
else if (page?.kind !== 'control') status.textContent = 'Benchmark request unavailable.';
// The control page intentionally creates no Stage, renderer, RAF or timer.
