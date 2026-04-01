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
    if (error.response?.status === 401) {
      useAuthStore.getState().logout();
      window.location.href = '/login';
      return Promise.reject(error);
    }
    const message = error.response?.data?.error
      || error.response?.statusText
      || error.message
      || 'An unexpected error occurred';
    const status = error.response?.status;
    error.userMessage = status >= 500
      ? 'Server error. Please try again later.'
      : message;
    error.statusCode = status;
    return Promise.reject(error);
  },
);

export default apiClient;
