import { afterEach, describe, expect, it, vi } from 'vitest';
import { get } from 'svelte/store';
import { readFileSync, readdirSync } from 'node:fs';
import { parse } from 'svelte/compiler';
import { catalogues, detectLocale, formatDate, formatNumber, interpolate, locale, localeStorageKey, resolveLocale, setLocale, t, translate } from './i18n';

afterEach(()=>{setLocale('en');vi.unstubAllGlobals();});
describe('Interface language',()=>{
  it('keeps explicit preferences and detects supported regional browser languages',()=>{
    expect(resolveLocale('fr',['de-DE'])).toBe('fr');
    expect(resolveLocale('bad',['pt-BR','es-MX'])).toBe('es');
    expect(detectLocale(['fr_CA'])).toBe('fr');
    expect(detectLocale(['de-AT'])).toBe('de');
    expect(detectLocale(['ja-JP'])).toBe('en');
    expect(detectLocale([])).toBe('en');
  });
  it('updates reactive labels and persists only a validated locale, even with unavailable storage',()=>{
    const setItem=vi.fn();vi.stubGlobal('localStorage',{setItem});
    setLocale('fr');expect(get(locale)).toBe('fr');expect(setItem).toHaveBeenLastCalledWith(localeStorageKey,'fr');
    expect(get(t)('Speichern')).not.toBe('Speichern');
    setLocale('de');expect(get(t)('Speichern')).toBe('Speichern');
    setLocale('bad' as never);expect(get(locale)).toBe('de');
    vi.stubGlobal('localStorage',{setItem(){throw new Error('blocked');}});
    expect(()=>setLocale('es')).not.toThrow();expect(get(locale)).toBe('es');
  });
  it('substitutes parameters once without translating or interpreting user content',()=>{
    expect(interpolate('Name: {name}; {count}; {missing}',{name:'<b>{count}</b>',count:2})).toBe('Name: <b>{count}</b>; 2; {missing}');
    const name='Speichern';
    expect(translate('Animation „{value1}“ ist mehrfach vorhanden. Verwende im Modell eindeutige Namen.',{value1:name},'en')).toContain(name);
    expect(translate('toString',{},'fr')).toBe('toString');
    expect(translate('__proto__',{},'es')).toBe('__proto__');
    expect(translate('A provider-specific diagnostic',{},'fr')).toBe('A provider-specific diagnostic');
  });
  it('localizes known dynamic host diagnostics while preserving technical detail',()=>{
    const result=translate('Systemstimme konnte nicht sprechen: network-error.',{},'en');
    expect(result).toContain('network-error');expect(result).not.toContain('konnte nicht sprechen');
  });
  it('localizes transport limits and reconnect advice while retaining diagnostics',()=>{
    for(const language of ['en','fr','es'] as const){
      expect(translate('Codex-Appserver stderr überschreitet die Größenbegrenzung',{},language)).not.toContain('Größenbegrenzung');
      const result=translate('MCP-Verbindung beendet: fixture-diagnostic. Server erneut verbinden.',{},language);
      expect(result).toContain('fixture-diagnostic');expect(result).not.toContain('erneut verbinden');
      expect(translate('MCP SSE enthält ungültiges JSON: line 1 column 7',{},language)).toContain('line 1 column 7');
    }
  });
  it('uses the selected locale for numbers and dates',()=>{
    expect(formatNumber(1234.5,'de')).toBe('1.234,5');expect(formatNumber(1234.5,'en')).toBe('1,234.5');
    expect(formatDate('2026-10-19T12:30:00Z',{timeZone:'UTC',year:'numeric',month:'2-digit',day:'2-digit'},'de')).toBe('19.10.2026');
    expect(formatDate('invalid',{},'es')).toBe('');
  });
});

const placeholders=(value:string)=>[...value.matchAll(/\{([A-Za-z][A-Za-z0-9_]*)\}/g)].map(match=>match[1]).sort();
describe('Translation coverage',()=>{
  it('has every catalogue key and named parameter in English, French and Spanish',()=>{
    for(const catalogue of catalogues){
      const keys=Object.keys(catalogue.en).sort();expect(Object.keys(catalogue.fr).sort()).toEqual(keys);expect(Object.keys(catalogue.es).sort()).toEqual(keys);
      for(const key of keys)for(const language of ['en','fr','es'] as const){
        expect(catalogue[language][key].trim(),`${language}: ${key}`).not.toBe('');
        expect(placeholders(catalogue[language][key]),`${language}: ${key}`).toEqual(placeholders(key));
      }
    }
  });
  it('covers each native glTF import diagnostic in every interface language',()=>{
    const source=readFileSync(new URL('../../src-tauri/src/gltf_import.rs',import.meta.url),'utf8').split('#[cfg(test)]')[0];
    const keys=new Set(catalogues.flatMap(catalogue=>Object.keys(catalogue.en)));
    const diagnostics=[...source.matchAll(/invalid\("((?:[^"\\]|\\.)*)"\)/g)];
    expect(diagnostics.length).toBeGreaterThan(50);
    for(const diagnostic of diagnostics){
      const text=JSON.parse('"'+diagnostic[1]+'"');
      expect(keys.has('glTF-Import: '+text),text).toBe(true);
    }
  });
  it('fully localizes every safe native credential diagnostic without translating arbitrary payloads',()=>{
    const source=readFileSync(new URL('../../src-tauri/src/credentials.rs',import.meta.url),'utf8');
    const reasons=source.match(/let reason = match error \{([\s\S]*?)\n\s*\};/)?.[1];
    expect(reasons,'native safe_error reason match').toBeDefined();
    const diagnostics=[...reasons!.matchAll(/=>\s*"((?:[^"\\]|\\.)*)"/g)].map(match=>
      'Sicherer Schlüsselspeicher: '+JSON.parse('"'+match[1]+'"'));
    expect(diagnostics.length).toBeGreaterThanOrEqual(8);
    for(const key of diagnostics)for(const language of ['en','fr','es'] as const){
      const catalogue=catalogues.find(catalogue=>Object.hasOwn(catalogue[language],key));
      expect(catalogue,`${language}: ${key}`).toBeDefined();
      const translated=translate(key,{},language);
      expect(translated).toBe(catalogue![language][key]);
      expect(translated).not.toContain(key.slice('Sicherer Schlüsselspeicher: '.length));
    }
    const payload='Ungültiges Datenformat: <viewer>{name}</viewer>';
    for(const language of ['en','fr','es'] as const){
      expect(translate('Sicherer Schlüsselspeicher: '+payload,{},language)).toContain(payload);
    }
  });
  it('covers literal translation calls across every Svelte view',()=>{
    const keys=new Set(catalogues.flatMap(catalogue=>Object.keys(catalogue.en)));
    const directory=new URL('../components/',import.meta.url);
    const files=[new URL('../App.svelte',import.meta.url),new URL('../Overlay.svelte',import.meta.url),...readdirSync(directory).filter(name=>name.endsWith('.svelte')).map(name=>new URL(name,directory))];
    for(const file of files){
      const ast=parse(readFileSync(file,'utf8'),{modern:true});
      function visit(node:any){if(!node||typeof node!=='object')return;
        if(node.type==='CallExpression'&&['$t','translate'].includes(node.callee?.name)&&typeof node.arguments?.[0]?.value==='string'){
          const key=node.arguments[0].value;expect(keys.has(key),`${file.pathname}: ${key}`).toBe(true);
        }
        for(const [key,value] of Object.entries(node)){if(['loc','metadata','parent'].includes(key))continue;if(Array.isArray(value))value.forEach(visit);else if(value&&typeof value==='object')visit(value);}
      }visit(ast);
    }
  });
});
