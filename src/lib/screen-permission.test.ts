import { describe, expect, it, vi } from 'vitest';
import { ScreenPermissionController } from './screen-permission';
describe('Screen permission lifecycle without OS dialogs or pixels',()=>{
  it('never prompts on initial, focus or manual status refreshes',async()=>{
    const invoke=vi.fn(async(_command:string)=>({platform:'macos',status:'notGranted'}));
    const publish=vi.fn();const controller=new ScreenPermissionController(invoke,publish);
    await controller.refresh();await controller.refresh();
    expect(invoke.mock.calls.map(args=>args[0])).toEqual(['get_screen_capture_permission','get_screen_capture_permission']);
    expect(publish.mock.lastCall![0].permission.status).toBe('notGranted');
  });
  it('coalesces explicit clicks while pending and trusts only a fresh valid result',async()=>{
    let complete!:(value:unknown)=>void;const invoke=vi.fn(()=>new Promise(resolve=>complete=resolve));const publish=vi.fn();const controller=new ScreenPermissionController(invoke,publish);
    const pending=controller.request();await controller.request();await controller.refresh();
    expect(invoke).toHaveBeenCalledTimes(1);expect(invoke).toHaveBeenCalledWith('request_screen_capture_permission');
    complete({platform:'macos',status:'notGranted'});await pending;
    expect(publish.mock.lastCall![0]).toMatchObject({loading:false,permission:{status:'notGranted'}});
  });
  it('discards late permission replies after leaving the panel and never retries a prompt',async()=>{
    let complete!:(value:unknown)=>void;const invoke=vi.fn(()=>new Promise(resolve=>complete=resolve));const publish=vi.fn();const controller=new ScreenPermissionController(invoke,publish);
    const pending=controller.request();controller.dispose();const calls=publish.mock.calls.length;
    complete({platform:'macos',status:'authorized'});await pending;await controller.request();
    expect(publish).toHaveBeenCalledTimes(calls);expect(invoke).toHaveBeenCalledTimes(1);
  });
  it('does not treat malformed or failed status checks as permission success',async()=>{
    for(const result of [{platform:'macos',status:'yes'},null,{status:'authorized'}]){
      const publish=vi.fn();const controller=new ScreenPermissionController(async()=>result,publish);await controller.refresh();
      expect(publish.mock.lastCall![0]).toMatchObject({loading:false,permission:null});expect(publish.mock.lastCall![0].error).toBeInstanceOf(Error);
    }
  });
  it('replaces an old success with an unknown status if a later refresh fails',async()=>{
    const invoke=vi.fn().mockResolvedValueOnce({platform:'macos',status:'authorized'}).mockRejectedValueOnce(new Error('Disconnected'));
    const publish=vi.fn();const controller=new ScreenPermissionController(invoke,publish);
    await controller.refresh();await controller.refresh();
    expect(publish.mock.lastCall![0]).toMatchObject({loading:false,permission:null});
    expect(publish.mock.lastCall![0].error.message).toBe('Disconnected');
    expect(invoke.mock.calls.every(args=>args[0]==='get_screen_capture_permission')).toBe(true);
  });
});
