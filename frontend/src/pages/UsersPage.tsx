import { useState } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { listUsers, createUser, deleteUser, updateUser } from '../api/users';
import { StatusBadge } from '../components/ui/StatusBadge';
import { TimeAgo } from '../components/ui/TimeAgo';
import { ConfirmDialog } from '../components/ui/ConfirmDialog';
import { toast } from 'sonner';
import type { UserRole } from '../types';

const roles: UserRole[] = ['super_admin', 'admin', 'operator', 'viewer'];

export default function UsersPage() {
  const qc = useQueryClient();
  const [showAdd, setShowAdd] = useState(false);
  const [username, setUsername] = useState('');
  const [password, setPassword] = useState('');
  const [displayName, setDisplayName] = useState('');
  const [role, setRole] = useState<UserRole>('viewer');
  const [delId, setDelId] = useState<string | null>(null);

  const { data, isLoading } = useQuery({ queryKey: ['users'], queryFn: listUsers });

  const addMut = useMutation({
    mutationFn: () => createUser({ username, password, display_name: displayName || undefined, role }),
    onSuccess: () => { toast.success('User created'); qc.invalidateQueries({ queryKey: ['users'] }); setShowAdd(false); setUsername(''); setPassword(''); setDisplayName(''); setRole('viewer'); },
    onError: () => toast.error('Failed to create user'),
  });

  const delMut = useMutation({
    mutationFn: (id: string) => deleteUser(id),
    onSuccess: () => { toast.success('User deleted'); qc.invalidateQueries({ queryKey: ['users'] }); setDelId(null); },
  });

  const toggleMut = useMutation({
    mutationFn: ({ id, active }: { id: string; active: boolean }) => updateUser(id, { active }),
    onSuccess: () => { toast.success('User updated'); qc.invalidateQueries({ queryKey: ['users'] }); },
  });

  const users = data?.users ?? [];

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <h2 className="text-2xl font-bold text-white">Users ({users.length})</h2>
        <button onClick={() => setShowAdd(true)} className="px-4 py-2 text-sm bg-blue-600 hover:bg-blue-700 text-white rounded-lg">Add User</button>
      </div>

      {isLoading ? <div className="text-slate-400">Loading...</div> : (
        <div className="bg-slate-900 border border-slate-700 rounded-xl overflow-hidden">
          <table className="w-full">
            <thead><tr className="border-b border-slate-700">
              <th className="px-4 py-3 text-left text-xs font-semibold text-slate-400 uppercase">Username</th>
              <th className="px-4 py-3 text-left text-xs font-semibold text-slate-400 uppercase">Display Name</th>
              <th className="px-4 py-3 text-left text-xs font-semibold text-slate-400 uppercase">Role</th>
              <th className="px-4 py-3 text-left text-xs font-semibold text-slate-400 uppercase">Status</th>
              <th className="px-4 py-3 text-left text-xs font-semibold text-slate-400 uppercase">Created</th>
              <th className="px-4 py-3 text-xs font-semibold text-slate-400 uppercase">Actions</th>
            </tr></thead>
            <tbody className="divide-y divide-slate-800">
              {users.map(u => (
                <tr key={u.id}>
                  <td className="px-4 py-3 text-sm text-slate-200">{u.username}</td>
                  <td className="px-4 py-3 text-sm text-slate-400">{u.display_name || '-'}</td>
                  <td className="px-4 py-3"><span className="text-xs px-2 py-0.5 rounded bg-slate-700 text-slate-300">{u.role}</span></td>
                  <td className="px-4 py-3"><StatusBadge status={u.active ? 'Connected' : 'Disconnected'} /></td>
                  <td className="px-4 py-3 text-sm"><TimeAgo date={u.created_at} className="text-slate-500" /></td>
                  <td className="px-4 py-3 text-center space-x-2">
                    <button onClick={() => toggleMut.mutate({ id: u.id, active: !u.active })} className="text-xs text-yellow-400 hover:text-yellow-300">{u.active ? 'Disable' : 'Enable'}</button>
                    <button onClick={() => setDelId(u.id)} className="text-xs text-red-400 hover:text-red-300">Delete</button>
                  </td>
                </tr>
              ))}
              {!users.length && <tr><td colSpan={6} className="px-4 py-8 text-center text-slate-500">No users</td></tr>}
            </tbody>
          </table>
        </div>
      )}

      {showAdd && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60">
          <div className="bg-slate-900 border border-slate-700 rounded-xl p-6 max-w-md w-full mx-4">
            <h3 className="text-lg font-semibold text-white mb-4">Add User</h3>
            <div className="space-y-3">
              <div><label className="block text-sm text-slate-300 mb-1">Username</label><input value={username} onChange={e => setUsername(e.target.value)} className="w-full px-3 py-2 bg-slate-800 border border-slate-600 rounded-lg text-white" /></div>
              <div><label className="block text-sm text-slate-300 mb-1">Password</label><input type="password" value={password} onChange={e => setPassword(e.target.value)} className="w-full px-3 py-2 bg-slate-800 border border-slate-600 rounded-lg text-white" /></div>
              <div><label className="block text-sm text-slate-300 mb-1">Display Name</label><input value={displayName} onChange={e => setDisplayName(e.target.value)} className="w-full px-3 py-2 bg-slate-800 border border-slate-600 rounded-lg text-white" /></div>
              <div><label className="block text-sm text-slate-300 mb-1">Role</label>
                <select value={role} onChange={e => setRole(e.target.value as UserRole)} className="w-full px-3 py-2 bg-slate-800 border border-slate-600 rounded-lg text-white">
                  {roles.map(r => <option key={r} value={r}>{r}</option>)}
                </select>
              </div>
            </div>
            <div className="flex justify-end gap-3 mt-6">
              <button onClick={() => setShowAdd(false)} className="px-4 py-2 text-sm text-slate-300 bg-slate-800 rounded-lg">Cancel</button>
              <button onClick={() => addMut.mutate()} disabled={!username || !password} className="px-4 py-2 text-sm bg-blue-600 text-white rounded-lg disabled:opacity-50">Create</button>
            </div>
          </div>
        </div>
      )}

      <ConfirmDialog open={!!delId} title="Delete User" message="This action cannot be undone." confirmLabel="Delete" variant="danger" onConfirm={() => delId && delMut.mutate(delId)} onCancel={() => setDelId(null)} />
    </div>
  );
}
