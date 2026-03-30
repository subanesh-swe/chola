import apiClient from './client';
import type { User, CreateUserRequest, UpdateUserRequest } from '../types';

export const listUsers = () =>
  apiClient.get<{ users: User[]; count: number }>('/users').then((r) => r.data);

export const getUser = (id: string) =>
  apiClient.get<User>(`/users/${id}`).then((r) => r.data);

export const createUser = (data: CreateUserRequest) =>
  apiClient.post<User>('/users', data).then((r) => r.data);

export const updateUser = (id: string, data: UpdateUserRequest) =>
  apiClient.put<User>(`/users/${id}`, data).then((r) => r.data);

export const deleteUser = (id: string) =>
  apiClient.delete(`/users/${id}`).then((r) => r.data);
