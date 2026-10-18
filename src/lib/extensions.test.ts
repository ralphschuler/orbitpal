import { describe, expect, it } from 'vitest';
import { mcpGrant, setMcpEnabled, setMcpTool, setSkillEnabled } from './extensions';

describe('global installation and per-bot grants', () => {
  it('grants neither tools nor viewer access when a library server is first enabled', () => {
    const definition = { id: 'obs', name: 'OBS', command: 'obs-server' };
    expect(mcpGrant([], definition.id)).toEqual({ id: 'obs', enabled: false, allowedTools: [], viewerAllowedTools: [] });
    expect(setMcpEnabled([], definition, true)).toEqual([{ ...definition, enabled: true, allowedTools: [], viewerAllowedTools: [] }]);
  });

  it('never grants a viewer tool that the bot has not been granted', () => {
    const original = [{ id: 'obs', enabled: true, allowedTools: [], viewerAllowedTools: [] }];
    const updated = setMcpTool(original, 'obs', 'change_scene', true, true);
    expect(mcpGrant(updated, 'obs').viewerAllowedTools).toEqual([]);
  });

  it('does not inherit authorization fields accidentally present in a global definition', () => {
    const imported = { id: 'obs', enabled: true, allowedTools: ['change_scene'], viewerAllowedTools: ['change_scene'] };
    expect(mcpGrant(setMcpEnabled([], imported, true), 'obs'))
      .toEqual({ id: 'obs', enabled: true, allowedTools: [], viewerAllowedTools: [] });
  });

  it('revokes viewer access together with bot access without mutating another bot', () => {
    const sharedDefinition = { id: 'obs', enabled: true, allowedTools: ['change_scene'], viewerAllowedTools: ['change_scene'] };
    const botA = [sharedDefinition];
    const botB = [sharedDefinition];
    const updatedA = setMcpTool(botA, 'obs', 'change_scene', false);
    expect(mcpGrant(updatedA, 'obs').viewerAllowedTools).toEqual([]);
    expect(mcpGrant(updatedA, 'obs').allowedTools).toEqual([]);
    expect(mcpGrant(botB, 'obs').allowedTools).toEqual(['change_scene']);
    expect(sharedDefinition.viewerAllowedTools).toEqual(['change_scene']);
  });

  it('keeps explicitly selected tool rights while disabling the server for one bot', () => {
    const original = [{ id: 'obs', enabled: true, allowedTools: ['read_scene'], viewerAllowedTools: [] }];
    const updated = setMcpEnabled(original, { id: 'obs' }, false);
    expect(mcpGrant(updated, 'obs')).toEqual({ id: 'obs', enabled: false, allowedTools: ['read_scene'], viewerAllowedTools: [] });
    expect(original[0].enabled).toBe(true);
  });

  it('normalizes malformed viewer grants to the bot permission subset', () => {
    const malformed = [{ id: 'obs', enabled: 'true', allowedTools: ['read', 'read'], viewerAllowedTools: ['read', 'write', null] }];
    expect(mcpGrant(malformed, 'obs')).toEqual({ id: 'obs', enabled: false, allowedTools: ['read'], viewerAllowedTools: ['read'] });
  });

  it('changes only the named server and does not invent an absent binding', () => {
    const original = [{ id: 'obs', enabled: true, allowedTools: [], viewerAllowedTools: [] }];
    expect(setMcpTool(original, 'missing', 'read', true)).toEqual(original);
    expect(setMcpTool(original, 'obs', 'read', true)[0].allowedTools).toEqual(['read']);
    expect(original[0].allowedTools).toEqual([]);
  });

  it('keeps skill installation independent from distinct bot assignments', () => {
    const botA = ['greeting'];
    const botB = ['greeting'];
    expect(setSkillEnabled(botA, 'greeting', false)).toEqual([]);
    expect(botB).toEqual(['greeting']);
    expect(setSkillEnabled(botA, 'greeting', true)).toEqual(['greeting']);
    expect(setSkillEnabled([], 'new-skill', true)).toEqual(['new-skill']);
  });
});
