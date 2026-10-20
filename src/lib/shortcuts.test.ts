import { describe, expect, it } from 'vitest';
import { shortcutFor, type ShortcutKey } from './shortcuts';
const context = { editing: false, modal: false, studio: true };
const event = (overrides: Partial<ShortcutKey>): ShortcutKey => ({ key: '', code: '', ctrlKey: false, metaKey: false, altKey: false, shiftKey: false, repeat: false, isComposing: false, ...overrides });
describe('keyboard actions respect input and dialog boundaries', () => {
  it('saves from editors on either desktop modifier', () => {
    expect(shortcutFor(event({ key: 's', ctrlKey: true }), { ...context, editing: true })).toBe('save');
    expect(shortcutFor(event({ key: 'S', metaKey: true }), { ...context, editing: true })).toBe('save');
  });
  it('dismisses a modal before a global stop', () => {
    expect(shortcutFor(event({ key: 'Escape' }), { ...context, modal: true })).toBe('dismiss');
    expect(shortcutFor(event({ key: 'Escape' }), context)).toBe('stop');
  });
  it('does not navigate while typing or using AltGr', () => {
    const keys = event({ code: 'Digit2', key: '2', altKey: true });
    expect(shortcutFor(keys, { ...context, editing: true })).toBeNull();
    expect(shortcutFor({ ...keys, ctrlKey: true }, context)).toBeNull();
  });
  it('supports macOS Option digit characters by physical key code', () => {
    expect(shortcutFor(event({ code: 'Digit1', key: '¡', altKey: true }), context)).toEqual({ page: 0 });
  });
  it('keeps question marks available inside text fields', () => {
    expect(shortcutFor(event({ key: '?', shiftKey: true }), { ...context, editing: true })).toBeNull();
    expect(shortcutFor(event({ key: '?' }), context)).toBe('help');
  });
  it('sends only from the control room and only once', () => {
    const key = event({ key: 'Enter', metaKey: true });
    expect(shortcutFor(key, context)).toBe('send');
    expect(shortcutFor(key, { ...context, studio: false })).toBeNull();
    expect(shortcutFor({ ...key, repeat: true }, context)).toBeNull();
  });
  it('never sends, navigates or stops during IME composition', () => {
    expect(shortcutFor(event({ key: 'Escape', isComposing: true }), context)).toBeNull();
    expect(shortcutFor(event({ key: 'Enter', ctrlKey: true, isComposing: true }), context)).toBeNull();
  });
  it('blocks page and send shortcuts behind a modal', () => {
    expect(shortcutFor(event({ code: 'Digit3', altKey: true }), { ...context, modal: true })).toBeNull();
    expect(shortcutFor(event({ key: 'Enter', ctrlKey: true }), { ...context, modal: true })).toBeNull();
  });
});

it('opens the ninth shared API access page with its physical digit shortcut',()=>{
  expect(shortcutFor({key:'9',code:'Digit9',ctrlKey:false,metaKey:false,altKey:true,shiftKey:false,repeat:false,isComposing:false},{editing:false,modal:false,studio:false})).toEqual({page:8});
  expect(shortcutFor({key:'9',code:'Digit9',ctrlKey:false,metaKey:false,altKey:true,shiftKey:false,repeat:false,isComposing:false},{editing:true,modal:false,studio:false})).toBeNull();
});
