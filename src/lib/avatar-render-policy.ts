export type RenderQuality = 'balanced' | 'energy';
export type AvatarRenderPolicy = { quality: RenderQuality; fps: 20 | 30 | 60; pixelRatio: number; shadows: boolean };

/** Rendering cost only. This never changes native simulation or action clocks. */
export function avatarRenderPolicy(output: unknown, devicePixelRatio: number, renderQuality?: RenderQuality): AvatarRenderPolicy {
  const value = output && typeof output === 'object' ? output as Record<string, unknown> : {};
  const quality = renderQuality === 'energy' ? 'energy' : 'balanced';
  const fps = Number(value.fps);
  const ratio = Number.isFinite(devicePixelRatio) && devicePixelRatio > 0 ? devicePixelRatio : 1;
  return { quality, fps: quality === 'energy' || fps === 20 ? 20 : fps === 30 ? 30 : 60,
    pixelRatio: Math.min(ratio, quality === 'energy' ? 1 : 1.75), shadows: quality !== 'energy' };
}

/** DOM exposure only; this does not claim native occlusion or visible pixels.
 * World actions keep their stricter measured surface/geometry gate. */
export function avatarSurfaceVisible(host: HTMLElement, canvas: HTMLCanvasElement, recovering = false): boolean {
  if (!host.isConnected || document.visibilityState === 'hidden' || host.clientWidth <= 0 || host.clientHeight <= 0) return false;
  const viewportWidth = window.innerWidth, viewportHeight = window.innerHeight;
  const rect = canvas.getBoundingClientRect();
  if (![rect.left, rect.top, rect.right, rect.bottom, viewportWidth, viewportHeight].every(Number.isFinite)
    || rect.right <= Math.max(0, rect.left) || rect.bottom <= Math.max(0, rect.top)
    || rect.left >= viewportWidth || rect.top >= viewportHeight) return false;
  let left = Math.max(0, rect.left), top = Math.max(0, rect.top);
  let right = Math.min(viewportWidth, rect.right), bottom = Math.min(viewportHeight, rect.bottom);
  let node: HTMLElement | null = canvas, count = 0;
  while (node && ++count <= 64) {
    const style = getComputedStyle(node);
    // A failed World mount intentionally hides this canvas until a successful
    // ordinary draw restores it. Ancestor visibility still gates that draw.
    if (style.display === 'none' || (!recovering || node !== canvas) && style.visibility === 'hidden'
      || style.visibility === 'collapse' || Number(style.opacity) <= 0) return false;
    if (node !== canvas && (['hidden', 'clip', 'scroll', 'auto'].includes(style.overflowX) || ['hidden', 'clip', 'scroll', 'auto'].includes(style.overflowY))) {
      const clip = node.getBoundingClientRect();
      if (![clip.left, clip.top, clip.right, clip.bottom].every(Number.isFinite)) return false;
      if (['hidden', 'clip', 'scroll', 'auto'].includes(style.overflowX)) {
        left = Math.max(left, clip.left); right = Math.min(right, clip.right);
      }
      if (['hidden', 'clip', 'scroll', 'auto'].includes(style.overflowY)) {
        top = Math.max(top, clip.top); bottom = Math.min(bottom, clip.bottom);
      }
      if (right <= left || bottom <= top) return false;
    }
    node = node.parentElement;
  }
  return node === null;
}
