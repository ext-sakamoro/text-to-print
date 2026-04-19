'use client';
import { useState, useEffect } from 'react';
import { useAdmin } from '@/lib/hooks/use-admin';

interface User {
  id: string;
  email: string;
  full_name: string | null;
  plan: string;
  role: string;
  banned: boolean;
  created_at: string;
}

export default function AdminUsersPage() {
  const { getUsers, updateUser } = useAdmin();
  const [users, setUsers] = useState<User[]>([]);
  const [loading, setLoading] = useState(true);

  const load = async () => {
    try { setUsers(await getUsers()); } catch { /* */ }
    setLoading(false);
  };

  useEffect(() => { load(); }, []);// eslint-disable-line react-hooks/exhaustive-deps

  const handlePlanChange = async (id: string, plan: string) => {
    await updateUser(id, { plan });
    setUsers((prev) => prev.map((u) => (u.id === id ? { ...u, plan } : u)));
  };

  const handleBan = async (id: string, banned: boolean) => {
    await updateUser(id, { banned });
    setUsers((prev) => prev.map((u) => (u.id === id ? { ...u, banned } : u)));
  };

  const handleRoleChange = async (id: string, role: string) => {
    await updateUser(id, { role });
    setUsers((prev) => prev.map((u) => (u.id === id ? { ...u, role } : u)));
  };

  return (
    <div className="p-6 space-y-4">
      <h1 className="text-2xl font-bold">Users ({users.length})</h1>
      {loading ? (
        <p className="text-sm text-muted-foreground">Loading...</p>
      ) : (
        <div className="overflow-x-auto">
          <table className="w-full text-sm">
            <thead>
              <tr className="border-b text-left text-muted-foreground">
                <th className="p-2">Email</th>
                <th className="p-2">Name</th>
                <th className="p-2">Plan</th>
                <th className="p-2">Role</th>
                <th className="p-2">Status</th>
                <th className="p-2">Joined</th>
                <th className="p-2">Actions</th>
              </tr>
            </thead>
            <tbody>
              {users.map((u) => (
                <tr key={u.id} className={`border-b ${u.banned ? 'bg-red-50 dark:bg-red-950' : ''}`}>
                  <td className="p-2 font-mono text-xs">{u.email}</td>
                  <td className="p-2">{u.full_name || '-'}</td>
                  <td className="p-2">
                    <select
                      value={u.plan}
                      onChange={(e) => handlePlanChange(u.id, e.target.value)}
                      className="text-xs border rounded px-1 py-0.5 bg-background"
                    >
                      {['Free', 'General', 'Pro', 'Enterprise'].map((p) => (
                        <option key={p} value={p}>{p}</option>
                      ))}
                    </select>
                  </td>
                  <td className="p-2">
                    <select
                      value={u.role || 'user'}
                      onChange={(e) => handleRoleChange(u.id, e.target.value)}
                      className="text-xs border rounded px-1 py-0.5 bg-background"
                    >
                      <option value="user">user</option>
                      <option value="admin">admin</option>
                    </select>
                  </td>
                  <td className="p-2">
                    {u.banned ? (
                      <span className="text-xs text-red-600 font-medium">BANNED</span>
                    ) : (
                      <span className="text-xs text-green-600">Active</span>
                    )}
                  </td>
                  <td className="p-2 text-xs text-muted-foreground">
                    {new Date(u.created_at).toLocaleDateString()}
                  </td>
                  <td className="p-2">
                    <button
                      onClick={() => handleBan(u.id, !u.banned)}
                      className={`text-xs px-2 py-0.5 rounded ${
                        u.banned
                          ? 'bg-green-100 text-green-700 hover:bg-green-200'
                          : 'bg-red-100 text-red-700 hover:bg-red-200'
                      }`}
                    >
                      {u.banned ? 'Unban' : 'Ban'}
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}
