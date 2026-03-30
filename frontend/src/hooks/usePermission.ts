import { useAuthStore } from '../stores/auth';
import type { UserRole } from '../types';

const roleHierarchy: Record<UserRole, number> = {
  super_admin: 4,
  admin: 3,
  operator: 2,
  viewer: 1,
};

export function usePermission() {
  const user = useAuthStore((s) => s.user);
  const role = user?.role ?? 'viewer';

  return {
    canManageUsers: role === 'super_admin',
    canManageRepos: roleHierarchy[role] >= roleHierarchy['admin'],
    canCancelJobs: roleHierarchy[role] >= roleHierarchy['admin'],
    canTriggerBuilds: roleHierarchy[role] >= roleHierarchy['operator'],
    canManageWorkers: roleHierarchy[role] >= roleHierarchy['admin'],
    hasMinRole: (minRole: UserRole) => roleHierarchy[role] >= roleHierarchy[minRole],
  };
}
