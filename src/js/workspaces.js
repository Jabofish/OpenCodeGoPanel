/**
 * Workspace display helpers and profile logic (P3).
 */

/**
 * Get display name for a workspace entry, respecting alias profile.
 */
export function getWorkspaceDisplayName(workspace, settings) {
  const profiles = settings?.workspaceProfiles || {};
  const profile = profiles[workspace?.id] || {};
  if (profile.alias) return profile.alias;
  return workspace?.name || workspace?.id || 'Unknown';
}

/**
 * Get workspace profile for a given workspace ID.
 */
export function getWorkspaceProfile(workspaceId, settings) {
  const profiles = settings?.workspaceProfiles || {};
  return profiles[workspaceId] || {};
}

/**
 * Sort workspaces: favorites first, then recent, then name.
 */
export function sortWorkspaces(workspaces, settings) {
  const profiles = settings?.workspaceProfiles || {};
  const recents = settings?.recentWorkspaces || [];
  return [...(workspaces || [])].sort((a, b) => {
    const fa = profiles[a.id]?.favorite ? 1 : 0;
    const fb = profiles[b.id]?.favorite ? 1 : 0;
    if (fa !== fb) return fb - fa;
    const ra = recents.indexOf(a.id);
    const rb = recents.indexOf(b.id);
    if (ra !== -1 && rb !== -1) return ra - rb;
    if (ra !== -1) return -1;
    if (rb !== -1) return 1;
    return (a.name || a.id).localeCompare(b.name || b.id);
  });
}
