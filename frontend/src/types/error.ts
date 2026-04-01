export interface ApiErrorResponse {
  error: string;
  status?: number;
}

export interface MutationError {
  userMessage?: string;
  message?: string;
  status?: number;
  statusCode?: number;
}
