import { afterEach, expect, it, vi } from 'vitest';

afterEach(() => {
  vi.doUnmock('./lib/avatar'); vi.doUnmock('@tauri-apps/api/core');
  vi.resetModules(); vi.unstubAllGlobals(); vi.useRealTimers();
});

function page(search = '?benchmark=figure&epoch=1') {
  const window = Object.assign(new EventTarget(), { location: { search } });
  const elements = new Map(['status', 'viewport', 'figure'].map(id => [id, { hidden: false, style: {}, textContent: '' }]));
  vi.stubGlobal('window', window);
  vi.stubGlobal('document', { getElementById: (id: string) => elements.get(id) });
  return window;
}
function deferred() {
  let resolve!: () => void;
  const promise = new Promise<void>(done => { resolve = done; });
  return { promise, resolve };
}
async function settle() { for (let n = 0; n < 20; n++) await Promise.resolve(); }

it('never constructs a Stage after pagehide wins the dynamic import', async () => {
  vi.resetModules(); vi.useFakeTimers(); const window = page();
  const imported = deferred(), importing = deferred(); const construct = vi.fn(), invoke = vi.fn();
  vi.doMock('@tauri-apps/api/core', () => ({ invoke }));
  vi.doMock('./lib/avatar', async () => {
    importing.resolve(); await imported.promise;
    return { AvatarStage: class { constructor() { construct(); } } };
  });
  await import('./native-render-benchmark'); await importing.promise;
  window.dispatchEvent(new Event('pagehide'));
  imported.resolve(); await settle();
  expect(construct).not.toHaveBeenCalled(); expect(invoke).not.toHaveBeenCalled();
  expect(vi.getTimerCount()).toBe(0);
});

it('disposes its retained Stage and starts no batch when pagehide wins loading', async () => {
  vi.resetModules(); vi.useFakeTimers(); const window = page();
  const loading = deferred(), constructed = deferred(); const dispose = vi.fn(), invoke = vi.fn();
  vi.doMock('@tauri-apps/api/core', () => ({ invoke }));
  vi.doMock('./lib/avatar', () => ({ AvatarStage: class {
    constructor() { constructed.resolve(); }
    loadAsset() { return loading.promise; }
    dispose = dispose;
  } }));
  await import('./native-render-benchmark'); await constructed.promise;
  window.dispatchEvent(new Event('pagehide'));
  expect(dispose).toHaveBeenCalledOnce();
  loading.resolve(); await settle();
  expect(dispose).toHaveBeenCalledOnce(); expect(invoke).not.toHaveBeenCalled();
  expect(vi.getTimerCount()).toBe(0);
});

it('uses the exact native-issued 20fps pilot setting without a profile or asset override', async () => {
  vi.resetModules(); vi.useFakeTimers(); const window = page('?benchmark=figure&epoch=6&fps=20');
  const constructed = deferred(); const construct = vi.fn(), dispose = vi.fn(), invoke = vi.fn();
  const loading = deferred();
  vi.doMock('@tauri-apps/api/core', () => ({ invoke }));
  vi.doMock('./lib/avatar', () => ({ AvatarStage: class {
    constructor(...args: unknown[]) { construct(...args); constructed.resolve(); }
    loadAsset() { return loading.promise; }
    dispose = dispose;
  } }));
  await import('./native-render-benchmark'); await constructed.promise;
  expect(construct.mock.calls[0][1]).toMatchObject({ profile: { id: 'benchmark-nova',
    output: { mode: 'studio', fps: 20 }, avatar: { assetPath: null } }, audioEnabled: false });
  window.dispatchEvent(new Event('pagehide')); loading.resolve(); await settle();
  expect(dispose).toHaveBeenCalledOnce(); expect(invoke).not.toHaveBeenCalled();
  expect(vi.getTimerCount()).toBe(0);
});
