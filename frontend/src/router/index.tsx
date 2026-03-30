import { Routes, Route } from 'react-router-dom';
import { ProtectedRoute } from './ProtectedRoute';
import { RoleGuard } from './RoleGuard';
import { Layout } from '../components/layout/Layout';
import LoginPage from '../pages/LoginPage';
import DashboardPage from '../pages/DashboardPage';
import BuildsPage from '../pages/BuildsPage';
import BuildDetailPage from '../pages/BuildDetailPage';
import WorkersPage from '../pages/WorkersPage';
import ReposPage from '../pages/ReposPage';
import RepoDetailPage from '../pages/RepoDetailPage';
import UsersPage from '../pages/UsersPage';
export function AppRouter() {
  return (
    <Routes>
      <Route path="/login" element={<LoginPage />} />
      <Route element={<ProtectedRoute />}>
        <Route element={<Layout />}>
          <Route index element={<DashboardPage />} />
          <Route path="builds" element={<BuildsPage />} />
          <Route path="builds/:id" element={<BuildDetailPage />} />
          <Route path="workers" element={<WorkersPage />} />
          <Route path="repos" element={<ReposPage />} />
          <Route path="repos/:id" element={<RepoDetailPage />} />
          <Route element={<RoleGuard minRole="super_admin" />}>
            <Route path="users" element={<UsersPage />} />
          </Route>
        </Route>
      </Route>
    </Routes>
  );
}
