import { Routes, Route } from 'react-router-dom';
import { ProtectedRoute } from './ProtectedRoute';
import { RoleGuard } from './RoleGuard';
import { Layout } from '../components/layout/Layout';
import LoginPage from '../pages/LoginPage';
import DashboardPage from '../pages/DashboardPage';
import BuildsPage from '../pages/BuildsPage';
import BuildDetailPage from '../pages/BuildDetailPage';
import BuildQueuePage from '../pages/BuildQueuePage';
import WorkersPage from '../pages/WorkersPage';
import ReposPage from '../pages/ReposPage';
import RepoDetailPage from '../pages/RepoDetailPage';
import UsersPage from '../pages/UsersPage';
import SettingsPage from '../pages/SettingsPage';
import AuditLogPage from '../pages/AuditLogPage';
import ProfilePage from '../pages/ProfilePage';
import AnalyticsPage from '../pages/AnalyticsPage';
import RunsPage from '../pages/RunsPage';
import BlacklistPage from '../pages/BlacklistPage';
import TokensPage from '../pages/TokensPage';
import LabelGroupsPage from '../pages/LabelGroupsPage';
import NotFoundPage from '../pages/NotFoundPage';

export function AppRouter() {
  return (
    <Routes>
      <Route path="/login" element={<LoginPage />} />
      <Route element={<ProtectedRoute />}>
        <Route element={<Layout />}>
          <Route index element={<DashboardPage />} />
          <Route path="builds" element={<BuildsPage />} />
          <Route path="builds/:id" element={<BuildDetailPage />} />
          <Route path="runs" element={<RunsPage />} />
          <Route path="queue" element={<BuildQueuePage />} />
          <Route path="workers" element={<WorkersPage />} />
          <Route path="repos" element={<ReposPage />} />
          <Route path="repos/:id" element={<RepoDetailPage />} />
          <Route path="analytics" element={<AnalyticsPage />} />
          <Route path="blacklist" element={<BlacklistPage />} />
          <Route path="tokens" element={<TokensPage />} />
          <Route path="label-groups" element={<LabelGroupsPage />} />
          <Route path="profile" element={<ProfilePage />} />
          <Route element={<RoleGuard minRole="super_admin" />}>
            <Route path="users" element={<UsersPage />} />
            <Route path="settings" element={<SettingsPage />} />
            <Route path="audit-log" element={<AuditLogPage />} />
          </Route>
        </Route>
      </Route>
      <Route path="*" element={<NotFoundPage />} />
    </Routes>
  );
}
