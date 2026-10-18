import type { Metadata } from 'next';

export const metadata: Metadata = {
  title: 'Manager Administration | Perigee',
  description: 'Manage white-label wealth managers on the Perigee platform.',
};

export default function AdminManagersPage() {
  return (
    <main className="p-6 max-w-4xl mx-auto">
      <h1 className="text-2xl font-bold text-gray-900 dark:text-gray-100">
        Manager Administration
      </h1>
      <p className="mt-2 text-gray-600 dark:text-gray-400">
        Manage white-label wealth managers and their client fleet allocations.
      </p>
      {/* TODO: wire up manager list from /api/v1/admin/managers */}
      <div className="mt-6 rounded-lg border border-gray-200 dark:border-gray-700 p-4">
        <p className="text-sm text-gray-500 dark:text-gray-400">
          No managers configured. Use the API to provision vault fleets.
        </p>
      </div>
    </main>
  );
}
