import { useNavigate } from 'react-router-dom';

export default function NotFoundPage() {
  const nav = useNavigate();
  return (
    <div className="min-h-screen flex items-center justify-center bg-app">
      <div className="text-center">
        <h1 className="text-6xl font-bold text-disabled">404</h1>
        <p className="text-lg text-muted mt-4">Page not found</p>
        <button onClick={() => nav('/')} className="mt-6 px-4 py-2 bg-accent text-on-accent rounded-lg text-sm hover:bg-accent-hover focus:outline-none focus:ring-2 focus:ring-accent transition-colors">
          Go to Dashboard
        </button>
      </div>
    </div>
  );
}
