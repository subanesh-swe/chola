import { useNavigate } from 'react-router-dom';

export default function NotFoundPage() {
  const nav = useNavigate();
  return (
    <div className="min-h-screen flex items-center justify-center bg-slate-950">
      <div className="text-center">
        <h1 className="text-6xl font-bold text-slate-600">404</h1>
        <p className="text-lg text-slate-400 mt-4">Page not found</p>
        <button onClick={() => nav('/')} className="mt-6 px-4 py-2 bg-blue-600 text-white rounded-lg text-sm">
          Go to Dashboard
        </button>
      </div>
    </div>
  );
}
