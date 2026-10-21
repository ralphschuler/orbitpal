import {describe,it,expect} from 'vitest';
import {lifeStartContext} from './life-start-context';

describe('Life start save context',()=>{
  it('keeps blank CLI model starts current when mandatory save supplies default',()=>{
    for(const provider of ['codex','claude-code'])for(const model of ['', '   ']){
      const profile={id:'nova',model:{provider,model,temperature:.8},output:{mode:'studio'}};
      const documents={agents:'instructions',persona:'persona'};
      const before=lifeStartContext(profile,documents);
      const saved={...profile,model:{...profile.model,model:'default'}};
      expect(lifeStartContext(saved,documents)).toBe(before);
      expect(profile.model.model).toBe(model);
    }
  });

  it('keeps explicit CLI models and blank HTTP models exact',()=>{
    for(const provider of ['codex','claude-code','compatible','ollama']){
      const profile={id:'nova',model:{provider,model:'specific-model'}};
      expect(lifeStartContext(profile,{})).toBe(JSON.stringify([profile,{}]));
    }
    const profile={id:'nova',model:{provider:'compatible',model:''}};
    expect(lifeStartContext(profile,{})).toBe(JSON.stringify([profile,{}]));
    expect(lifeStartContext(profile,{})).not.toBe(lifeStartContext({...profile,model:{...profile.model,model:'default'}},{}));
  });

  it('still invalidates actual profile, destination, model and document edits',()=>{
    const profile={id:'nova',model:{provider:'codex',model:'',baseUrl:'',temperature:.8},screenContext:{enabled:false}};
    const documents={agents:'instructions',persona:'persona'};
    const before=lifeStartContext(profile,documents);
    for(const changed of [
      {...profile,id:'other'},
      {...profile,model:{...profile.model,provider:'claude-code'}},
      {...profile,model:{...profile.model,model:'chosen-model'}},
      {...profile,model:{...profile.model,baseUrl:'http://127.0.0.1:9/v1'}},
      {...profile,model:{...profile.model,temperature:.5}},
      {...profile,screenContext:{enabled:true}},
    ])expect(lifeStartContext(changed,documents)).not.toBe(before);
    expect(lifeStartContext(profile,{...documents,agents:'edited'})).not.toBe(before);
  });
});
