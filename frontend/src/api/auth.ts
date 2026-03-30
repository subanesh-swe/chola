import apiClient from './client';
import type { LoginRequest, LoginResponse, User } from '../types';

export const login = (data: LoginRequest) =>
  apiClient.post<LoginResponse>('/auth/login', data).then((r) => r.data);

export const logout = () =>
  apiClient.post('/auth/logout').then((r) => r.data);

export const getMe = () =>
  apiClient.get<User>('/auth/me').then((r) => r.data);
