/** Native window visibility is physical state. Persisted output settings arrive
 * through the separate profile event, including ordinary configured overlays. */
export interface OverlayVisibilityTarget {
  botId: string;
  setLive(botId:string,active:boolean):void;
  stopPlayer():void;
  playbackIdle():void;
}
export function applyOverlayVisibility(value:unknown,target:OverlayVisibilityTarget):void {
  if(!value||typeof value!=='object'||Array.isArray(value))return;
  const event=value as Record<string,unknown>;
  const botId=event.botId,active=event.active;
  if(typeof botId!=='string'||!/^[A-Za-z0-9_-]{1,96}$/.test(botId)||typeof active!=='boolean')return;
  target.setLive(botId,active);
  if(botId!==target.botId)return;
  if(active)target.stopPlayer();else target.playbackIdle();
}
