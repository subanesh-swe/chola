import { Navigate, Outlet } from 'react-router-dom';
import { useAuthStore } from '../stores/auth';
import type { UserRole } from '../types';

const roleHierarchy: Record<UserRole, number> = {
  super_admin: 4,
  admin: 3,
  operator: 2,
  viewer: 1,
};

interface Props {
  minRole: UserRole;
}

export function RoleGuard({ minRole }: Props) {
  const user = useAuthStore((s) => s.user);
  if (!user || roleHierarchy[user.role] < roleHierarchy[minRole]) {
    return <Navigate to="/" replace />;
  }
  return <Outlet />;
}
