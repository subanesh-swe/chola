import { Component, type ReactNode } from 'react';

interface Props {
  children: ReactNode;
  fallback?: ReactNode;
}

interface State {
  hasError: boolean;
  error: Error | null;
}

export class ErrorBoundary extends Component<Props, State> {
  constructor(props: Props) {
    super(props);
    this.state = { hasError: false, error: null };
  }

  static getDerivedStateFromError(error: Error): State {
    return { hasError: true, error };
  }

  render() {
    if (this.state.hasError) {
      return this.props.fallback || (
        <div className="min-h-screen flex items-center justify-center bg-app">
          <div className="bg-surface border border-danger/30 rounded-xl p-8 max-w-md">
            <h2 className="text-xl font-bold text-danger mb-2">Something went wrong</h2>
            <p className="text-sm text-muted mb-4">{this.state.error?.message || 'An unexpected error occurred'}</p>
            <button
              onClick={() => { this.setState({ hasError: false, error: null }); window.location.href = '/'; }}
              className="px-4 py-2 bg-accent text-on-accent rounded-lg text-sm hover:bg-accent-hover focus:outline-none focus:ring-2 focus:ring-accent transition-colors"
            >
              Go to Dashboard
            </button>
          </div>
        </div>
      );
    }
    return this.props.children;
  }
}
