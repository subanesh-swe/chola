import axios from 'axios';
import { useAuthStore } from '../stores/auth';

const apiClient = axios.create({
  baseURL: '/api/v1',
  headers: {
    'Content-Type': 'application/json',
    'X-Requested-With': 'XMLHttpRequest',
  },
  timeout: 30000,
});

apiClient.interceptors.request.use((config) => {
  const { token, isTokenExpired, logout } = useAuthStore.getState();

  if (token && isTokenExpired()) {
    logout();
    window.location.href = '/login';
    return Promise.reject(new Error('Token expired'));
  }

  if (token) {
    config.headers.Authorization = `Bearer ${token}`;
  }
  return config;
});

apiClient.interceptors.response.use(
  (response) => response,
  (error) => {
    const status = error.response?.status;
    const url: string = error.config?.url || '';
    // A 401 from the login endpoint means "bad credentials" — let the caller
    // (LoginPage) show the error. Only treat 401 on OTHER endpoints as an
    // expired/invalid session that should log out and redirect. Also skip the
    // hard redirect when already on /login so we don't reload over the toast.
    const isAuthAttempt = url.includes('/auth/login');
    if (status === 401 && !isAuthAttempt) {
      useAuthStore.getState().logout();
      if (window.location.pathname !== '/login') {
        window.location.href = '/login';
      }
      return Promise.reject(error);
    }
    const message = error.response?.data?.error
      || error.response?.statusText
      || error.message
      || 'An unexpected error occurred';
    error.userMessage = status && status >= 500
      ? 'Server error. Please try again later.'
      : message;
    error.statusCode = status;
    return Promise.reject(error);
  },
);

export default apiClient;
