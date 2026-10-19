import type { Metadata } from 'next';

export const metadata: Metadata = {
  title: 'Managers | Perigee',
  description: 'Browse registered wealth managers on the Perigee platform.',
};

export default function ManagersPage() {
  return (
    <main className="p-6 max-w-4xl mx-auto">
      <h1 className="text-2xl font-bold text-gray-900 dark:text-gray-100">
        Managers
      </h1>
      <p className="mt-2 text-gray-600 dark:text-gray-400">
        Browse registered wealth managers offering Perigee-powered portfolios.
      </p>
      {/* TODO: wire up manager list from /api/v1/managers */}
      <div className="mt-6 rounded-lg border border-gray-200 dark:border-gray-700 p-4">
        <p className="text-sm text-gray-500 dark:text-gray-400">
          No managers found. Check back later.
        </p>
      </div>
    </main>
  );
}
