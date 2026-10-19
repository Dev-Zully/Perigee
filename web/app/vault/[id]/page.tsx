interface VaultPageProps {
  params: { id: string };
}

export default function VaultPage({ params }: VaultPageProps) {
  return (
    <div className="p-6">
      <h1 className="text-2xl font-bold">Vault {params.id}</h1>
      <p className="mt-2 text-gray-600 dark:text-gray-400">
        Vault detail view for vault ID: {params.id}
      </p>
    </div>
  );
}

export async function generateStaticParams() {
  return [];
}
