import { afterEach, describe, expect, it, vi } from 'vitest';
import { avatarRenderPolicy, avatarSurfaceVisible } from './avatar-render-policy';

afterEach(() => vi.unstubAllGlobals());
describe('Avatar rendering cost policy', () => {
  it('retains existing quality for old or malformed profiles and bounds pixel density', () => {
    for (const output of [undefined, null, false, {}, { renderQuality: 'unknown' }]) {
      expect(avatarRenderPolicy(output, 3)).toEqual({ quality: 'balanced', fps: 60, pixelRatio: 1.75, shadows: true });
    }
    for (const ratio of [NaN, Infinity, -1, 0]) expect(avatarRenderPolicy({}, ratio).pixelRatio).toBe(1);
    expect(avatarRenderPolicy({}, .8).pixelRatio).toBe(.8);
    // Imported profile fields cannot carry the separate installation setting.
    expect(avatarRenderPolicy({renderQuality:'energy',fps:60},2).quality).toBe('balanced');
  });
  it('keeps the selected cadence in balanced mode and explicitly caps the energy mode', () => {
    for (const fps of [20, 30, 60] as const) {
      expect(avatarRenderPolicy({ fps }, 2).fps).toBe(fps);
      expect(avatarRenderPolicy({ fps }, 2, 'energy'))
        .toEqual({ quality: 'energy', fps: 20, pixelRatio: 1, shadows: false });
    }
  });
});

function surface() {
  vi.stubGlobal('document', { visibilityState: 'visible' });
  vi.stubGlobal('window', { innerWidth: 1000, innerHeight: 800 });
  const styles = new Map<unknown, Record<string, string>>();
  vi.stubGlobal('getComputedStyle', (node: unknown) => ({ display: 'block', visibility: 'visible', opacity: '1', overflowX: 'visible', overflowY: 'visible', ...styles.get(node) }));
  const rect = { left: 10, top: 20, right: 210, bottom: 220 };
  const host = { isConnected: true, clientWidth: 200, clientHeight: 200, parentElement: null, getBoundingClientRect: () => rect };
  const canvas = { parentElement: host, getBoundingClientRect: () => rect };
  const visible = () => avatarSurfaceVisible(host as unknown as HTMLElement, canvas as unknown as HTMLCanvasElement);
  return { host, canvas, rect, styles, visible };
}
describe('DOM-only GPU suppression', () => {
  it('draws intersecting outputs and suppresses detached, hidden, zero-size and out-of-view surfaces', () => {
    const s = surface();
    expect(s.visible()).toBe(true);
    s.host.isConnected = false; expect(s.visible()).toBe(false); s.host.isConnected = true;
    s.host.clientWidth = 0; expect(s.visible()).toBe(false); s.host.clientWidth = 200;
    vi.stubGlobal('document', { visibilityState: 'hidden' }); expect(s.visible()).toBe(false);
    vi.stubGlobal('document', { visibilityState: 'visible' });
    s.rect.left = 1001; s.rect.right = 1201; expect(s.visible()).toBe(false);
    s.rect.left = -199; s.rect.right = 1; expect(s.visible()).toBe(true);
    s.rect.right = 0; expect(s.visible()).toBe(false);
  });
  it('honors hidden ancestors and only clips the axes actually constrained by CSS', () => {
    const s = surface();
    s.styles.set(s.host, { opacity: '0' }); expect(s.visible()).toBe(false);
    s.styles.set(s.host, { visibility: 'hidden' }); expect(s.visible()).toBe(false);
    s.styles.set(s.host, { display: 'none' }); expect(s.visible()).toBe(false);
    s.host.getBoundingClientRect = () => ({ left: 300, top: 20, right: 500, bottom: 220 });
    s.styles.set(s.host, { overflowX: 'hidden' }); expect(s.visible()).toBe(false);
    s.styles.set(s.host, { overflowY: 'hidden' }); expect(s.visible()).toBe(true);
    s.styles.clear(); expect(s.visible()).toBe(true);
  });
  it.each(['x', 'y'])('intersects cumulative ancestor clips on the %s axis', axis => {
    const s = surface();
    Object.assign(s.rect, { left: 0, top: 0, right: 800, bottom: 800 });
    const first = { left: 0, top: 0, right: axis === 'x' ? 100 : 800, bottom: axis === 'y' ? 100 : 800 };
    const second = { left: axis === 'x' ? 700 : 0, top: axis === 'y' ? 700 : 0, right: 800, bottom: 800 };
    const grandparent = { parentElement: null, getBoundingClientRect: () => second };
    s.host.getBoundingClientRect = () => first;
    (s.host as unknown as { parentElement: unknown }).parentElement = grandparent;
    s.styles.set(s.host, { [axis === 'x' ? 'overflowX' : 'overflowY']: 'hidden' });
    s.styles.set(grandparent, { [axis === 'x' ? 'overflowX' : 'overflowY']: 'hidden' });
    expect(s.visible()).toBe(false); // Both clips intersect the canvas, but not one another.
    if (axis === 'x') second.left = 50; else second.top = 50;
    expect(s.visible()).toBe(true);
  });
  it('intersects ancestor clips with the remaining viewport and refuses unknown geometry', () => {
    const s = surface();
    s.rect.left = 900; s.rect.right = 1100;
    const clip = { left: 1050, top: 0, right: 1150, bottom: 800 };
    s.host.getBoundingClientRect = () => clip;
    s.styles.set(s.host, { overflowX: 'hidden' });
    expect(s.visible()).toBe(false);
    clip.left = 950; expect(s.visible()).toBe(true);
    clip.left = NaN; expect(s.visible()).toBe(false);
  });
  it('refuses an unbounded ancestor chain', () => {
    const s = surface();
    (s.host as unknown as { parentElement: unknown }).parentElement = s.host;
    expect(s.visible()).toBe(false);
  });
  it('permits only the recovery canvas hiding flag while still checking its ancestors', () => {
    const s = surface();
    s.styles.set(s.canvas, { visibility: 'hidden' });
    expect(s.visible()).toBe(false);
    const recover = () => avatarSurfaceVisible(s.host as unknown as HTMLElement, s.canvas as unknown as HTMLCanvasElement, true);
    expect(recover()).toBe(true);
    s.styles.set(s.host, { visibility: 'hidden' }); expect(recover()).toBe(false);
    s.styles.delete(s.host); s.styles.set(s.canvas, { display: 'none' }); expect(recover()).toBe(false);
  });
});
