// SPDX-License-Identifier: MIT
/** Read-only common object layer. Parsed native replies and confirmed static World
 * config are joined here; this never admits native Ready, movement or an ACK. */
import type {NativeWorldView} from './native-world-output';
import {sameNativeSnapshotBinding,type NativeObjectConfig} from './native-world-snapshots';
import type {CssSize} from './world-output';
import type {WorldAnchor} from './world-state';
import {worldObjectAnchorsForActor} from './world-object-owner';

export function nativeWorldObjectAnchors(view:NativeWorldView,data:NativeObjectConfig|null,
  viewport:CssSize,documentVisible:boolean):readonly WorldAnchor[]{
  const binding=view.binding;
  if(!documentVisible||view.phase!=='world'||!view.exposed||view.covered||view.challenge||!binding
    ||binding.cameraPolicy!=='native_physical_iso_v1'||!data||!sameNativeSnapshotBinding(binding,data.binding)
    ||data.instanceId!==binding.worldInstanceId||data.stageId!==binding.stageId
    ||data.configRevision!==binding.configRevision
    ||!data.actors.some(actor=>actor.actorId===binding.actorId))return [];
  // WorldObjects is inset:0 in the same full content viewport as Avatar. Both
  // host and renderer map canonical/1e6 * this CSS viewport. Safe regions limit
  // reachable centers; neither model footprint nor DPR remaps public anchors.
  const framing=binding.nativeFraming;
  if(!framing||![viewport.width,viewport.height].every(n=>Number.isFinite(n)&&n>0)
    ||Math.abs(viewport.width-binding.viewport.width)>.5||Math.abs(viewport.height-binding.viewport.height)>.5
    ||Math.abs(viewport.width-framing.physicalWidth/framing.scaleFactor)>.5
    ||Math.abs(viewport.height-framing.physicalHeight/framing.scaleFactor)>.5)return [];
  return worldObjectAnchorsForActor(data,binding.actorId,data.anchors);
}
