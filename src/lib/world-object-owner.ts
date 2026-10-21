// SPDX-License-Identifier: MIT
import type {WorldAnchor} from './world-state';

export type WorldObjectMembership=Readonly<{actors:readonly Readonly<{actorId:string}>[]}>;

/** Read-only display role for one exact World instance/stage/config revision.
 * The native snapshot includes every configured member, including life:null.
 * Readiness, current activity and selected profile never choose a successor.
 * This role grants no native ownership, action or completion authority. */
export function designatedWorldObjectActor(snapshot:WorldObjectMembership):string|null{
  let designated:string|null=null;
  for(const actor of snapshot.actors){
    if(designated===null||actor.actorId<designated)designated=actor.actorId;
  }
  return designated;
}

/** Filter an already confirmed read-only layer. This cannot make its input
 * anchors current, visible or authoritative; callers retain those checks. */
export function worldObjectAnchorsForActor(snapshot:WorldObjectMembership,actorId:string,anchors:readonly WorldAnchor[]):readonly WorldAnchor[]{
  return designatedWorldObjectActor(snapshot)===actorId?anchors:[];
}
