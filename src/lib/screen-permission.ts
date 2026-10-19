export interface ScreenPermission { platform: string; status: 'authorized'|'notGranted'|'systemManaged' }
export interface PermissionState { permission: ScreenPermission|null; loading: boolean; error: unknown }
type Command = 'get_screen_capture_permission'|'request_screen_capture_permission';

/** Refreshes never prompt. Only an explicit request() invokes the OS prompt;
 * neither method enables any bot's screen/model grant or starts a capture. */
export class ScreenPermissionController {
  private revision=0;
  private disposed=false;
  private state: PermissionState={permission:null,loading:false,error:null};
  constructor(private invoke:(command:Command)=>Promise<unknown>,private publish:(state:PermissionState)=>void) {}
  refresh(){return this.run('get_screen_capture_permission');}
  request(){return this.run('request_screen_capture_permission');}
  dispose(){this.disposed=true;++this.revision;}
  private async run(command:Command):Promise<void>{
    if(this.disposed||this.state.loading)return;
    const revision=++this.revision;
    this.state={...this.state,loading:true,error:null};this.publish(this.state);
    try{
      const result=await this.invoke(command);
      if(this.disposed||revision!==this.revision)return;
      if(!result||typeof result!=='object'||typeof (result as any).platform!=='string'||!['authorized','notGranted','systemManaged'].includes((result as any).status))throw new Error('Die Desktop-App hat keinen gültigen Bildschirmfreigabestatus geliefert.');
      this.state={...this.state,permission:result as ScreenPermission};
    }catch(error){if(!this.disposed&&revision===this.revision)this.state={...this.state,permission:null,error};}
    finally{if(!this.disposed&&revision===this.revision){this.state={...this.state,loading:false};this.publish(this.state);}}
  }
}
