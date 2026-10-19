import { describe, expect, it } from 'vitest';
import { countdown, speechBlocker, speechBlockerMessages, speechStatus, speechStatusMessages, type SpeechBlocker, type SpeechStatusInput } from './autopilot-status';
import catalogue from './locales/autopilot-status.json';
const input:SpeechStatusInput={active:true,speak:true,outputAvailable:true,modelConfigured:true,
  schedule:{eligible:true,inFlight:false,dueAt:180_000,remainingSeconds:145,budgetLimited:false,attempts:0,limit:12}};

describe('Visible Autopilot speech reasons',()=>{
  it.each(Object.keys(speechBlockerMessages) as SpeechBlocker[])('uses %s as a real blocking guard and display reason',reason=>{
    expect(speechBlocker({[reason]:true})).toBe(reason);
    expect(speechStatus({...input,blocked:reason})).toEqual({message:speechBlockerMessages[reason],waiting:true});
  });
  it('prioritizes actual blockers and does not treat absent or false flags as activity',()=>{
    expect(speechBlocker({unsaved:true,draft:true,reply:true,update:true})).toBe('update');
    expect(speechBlocker({unsaved:true,draft:true})).toBe('unsaved');
    expect(speechBlocker({draft:true})).toBe('draft');
    expect(speechBlocker({update:false,unsaved:false})).toBeUndefined();
    expect(speechBlocker({})).toBeUndefined();
  });
  it('distinguishes disabled speech, missing model/output, preparation and quiet countdown',()=>{
    expect(speechStatus({...input,active:false}).message).toBe(speechStatusMessages.inactive);
    expect(speechStatus({...input,speak:false}).message).toBe(speechStatusMessages.disabled);
    expect(speechStatus({...input,modelConfigured:false}).message).toBe(speechStatusMessages.model);
    expect(speechStatus({...input,outputAvailable:false}).message).toBe(speechStatusMessages.output);
    expect(speechStatus({...input,schedule:{...input.schedule!,inFlight:true},blocked:'reply'}).message).toBe(speechStatusMessages.working);
    expect(speechStatus({...input,schedule:{...input.schedule!,waitingForBot:'other'}}).message).toBe(speechStatusMessages.otherBot);
    expect(speechStatus({...input,schedule:undefined}).message).toBe(speechStatusMessages.initializing);
    expect(speechStatus(input)).toMatchObject({message:speechStatusMessages.countdown,parameters:{time:'2:25'},waiting:false});
    expect(speechStatus({...input,schedule:{...input.schedule!,remainingSeconds:0}}).message).toBe(speechStatusMessages.ready);
  });
  it('shows hourly limits before the actual budget expires without claiming speech will occur',()=>{
    expect(speechStatus({...input,schedule:{...input.schedule!,budgetLimited:true,remainingSeconds:3600}}))
      .toEqual({message:speechStatusMessages.budget,parameters:{time:'60:00'},waiting:true});
    expect(speechStatus({...input,schedule:{...input.schedule!,budgetLimited:true},blocked:'recording'}).message).toBe(speechBlockerMessages.recording);
  });
  it('formats countdown boundaries without negative, nonfinite or fractional seconds',()=>{
    expect([0,1,59,60,120.1,-5,Infinity,NaN].map(countdown)).toEqual(['0:00','0:01','0:59','1:00','2:01','0:00','0:00','0:00']);
  });
  it('includes every controlled status in the three translated catalogues',()=>{
    for(const language of ['en','fr','es'] as const){
      for(const message of [...Object.values(speechBlockerMessages),...Object.values(speechStatusMessages)]){
        expect((catalogue[language] as Record<string,string>)[message]?.trim(),`${language}: ${message}`).toBeTruthy();
      }
    }
  });
});
