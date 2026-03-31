import apiClient from './client';
import type { User, CreateUserRequest, UpdateUserRequest } from '../types';

interface PaginatedResponse<T> {
  data: T[];
  pagination: { total: number; limit: number; offset: number };
}

export const listUsers = (params?: { limit?: number; offset?: number }) =>
  apiClient.get<PaginatedResponse<User>>('/users', { params }).then((r) => r.data);

export const getUser = (id: string) =>
  apiClient.get<User>(`/users/${id}`).then((r) => r.data);

export const createUser = (data: CreateUserRequest) =>
  apiClient.post<User>('/users', data).then((r) => r.data);

export const updateUser = (id: string, data: UpdateUserRequest) =>
  apiClient.put<User>(`/users/${id}`, data).then((r) => r.data);

export const deleteUser = (id: string) =>
  apiClient.delete(`/users/${id}`).then((r) => r.data);
