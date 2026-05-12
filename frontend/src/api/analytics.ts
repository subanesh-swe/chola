import apiClient from './client';
import type { AnalyticsData } from '../types';
import type { BuildFilters } from '../hooks/useUrlFilters';

interface AnalyticsQueryParams {
  from?: string;
  to?: string;
  repo_id?: string;
  branch?: string;
  stage_name?: string;
  exit_code?: number;
  granularity?: string;
}

// `granularity` may not be in BuildFilters yet (owned by another workstream).
// Read it defensively so this file works regardless of when the field lands.
type AnalyticsFilters = Partial<BuildFilters> & { granularity?: string };

function filtersToAnalyticsParams(filters: AnalyticsFilters): AnalyticsQueryParams {
  const params: AnalyticsQueryParams = {};
  if (filters.dateFrom) params.from = filters.dateFrom;
  if (filters.dateTo) params.to = filters.dateTo;
  if (filters.repo) params.repo_id = filters.repo;
  if (filters.branch) params.branch = filters.branch;
  if (filters.stage) params.stage_name = filters.stage;
  if (filters.exitCode) {
    params.exit_code = filters.exitCode === 'nonzero' ? -1 : Number(filters.exitCode);
  }
  if (filters.granularity) params.granularity = filters.granularity;
  return params;
}

export const getAnalytics = (filters: AnalyticsFilters = {}) => {
  const params = filtersToAnalyticsParams(filters);
  return apiClient.get<AnalyticsData>('/analytics', { params }).then((r) => r.data);
};
