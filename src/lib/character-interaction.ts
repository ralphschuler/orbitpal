import type {InteractionKind} from './experience';

export interface CharacterInteractionRequest {kind:InteractionKind;targetActorId:string|null}

/** A peer selection is only an operator request; native admission owns availability. */
export function characterInteractionRequest(botId:string,kind:InteractionKind,targetActorId:string|null,peers:readonly {id:string}[]):CharacterInteractionRequest {
  if(kind!=='play')return {kind,targetActorId:null};
  if(!targetActorId||targetActorId===botId||!/^[A-Za-z0-9_-]{1,96}$/.test(targetActorId)
    ||!peers.some(peer=>peer.id===targetActorId))throw Error('experience_invalid');
  return {kind,targetActorId};
}
