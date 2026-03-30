import apiClient from './client';
import type { DashboardSummary } from '../types';

export const getDashboardSummary = () =>
  apiClient.get<DashboardSummary>('/dashboard/summary').then((r) => r.data);
