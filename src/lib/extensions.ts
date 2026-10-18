export interface McpGrant {
  id: string;
  enabled: boolean;
  allowedTools: string[];
  viewerAllowedTools: string[];
  [key: string]: unknown;
}

const names = (value: unknown): string[] => Array.isArray(value)
  ? [...new Set(value.filter((item): item is string => typeof item === 'string' && Boolean(item.trim())))] : [];

/** Read access from this bot only; catalog installation grants no access. */
export function mcpGrant(servers: unknown, id: string): McpGrant {
  const current = Array.isArray(servers) ? servers.find((server) => server?.id === id) : undefined;
  const allowedTools = names(current?.allowedTools);
  return {
    id, enabled: current?.enabled === true, allowedTools,
    viewerAllowedTools: names(current?.viewerAllowedTools).filter((tool) => allowedTools.includes(tool)),
  };
}

export function setMcpEnabled(servers: unknown, definition: { id: string; [key: string]: unknown }, enabled: boolean): McpGrant[] {
  const current = Array.isArray(servers) ? servers : [];
  const grant = { ...mcpGrant(current, definition.id), enabled };
  const found = current.some((server) => server?.id === definition.id);
  return found
    ? current.map((server) => server?.id === definition.id ? { ...server, ...grant } : server)
    : [...current, { ...definition, ...grant }];
}

/** Viewer access is always a subset of the bot's explicit tool access. */
export function setMcpTool(servers: unknown, id: string, tool: string, checked: boolean, viewer = false): McpGrant[] {
  const current = Array.isArray(servers) ? servers : [];
  const grant = mcpGrant(current, id);
  if (!tool.trim() || !current.some((server) => server?.id === id)) return [...current];
  const allowed = new Set(grant.allowedTools);
  const viewers = new Set(grant.viewerAllowedTools);
  if (viewer) {
    if (checked && allowed.has(tool)) viewers.add(tool);
    else viewers.delete(tool);
  } else if (checked) allowed.add(tool);
  else { allowed.delete(tool); viewers.delete(tool); }
  const next = { ...grant, allowedTools: [...allowed], viewerAllowedTools: [...viewers].filter((name) => allowed.has(name)) };
  return current.map((server) => server?.id === id ? { ...server, ...next } : server);
}

export function setSkillEnabled(skills: unknown, id: string, enabled: boolean): string[] {
  const current = names(skills);
  return enabled ? [...new Set([...current, id])] : current.filter((skill) => skill !== id);
}
