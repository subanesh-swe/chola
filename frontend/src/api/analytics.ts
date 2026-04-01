import apiClient from './client';
import type { AnalyticsData } from '../types';

export const getAnalytics = (days: number = 30) =>
  apiClient.get<AnalyticsData>('/analytics', { params: { days } }).then((r) => r.data);
