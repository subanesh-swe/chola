export type UserRole = 'super_admin' | 'admin' | 'operator' | 'viewer';

export interface User {
  id: string;
  username: string;
  display_name: string | null;
  role: UserRole;
  active: boolean;
  created_at: string;
  updated_at: string;
}

export interface LoginRequest {
  username: string;
  password: string;
}

export interface LoginResponse {
  token: string;
  expires_at: string;
  user: User;
}

export interface CreateUserRequest {
  username: string;
  password: string;
  display_name?: string;
  role: UserRole;
}

export interface UpdateUserRequest {
  display_name?: string;
  role?: UserRole;
  active?: boolean;
}
