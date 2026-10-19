interface VaultsPageProps {
  params: { id: string };
}

export default function VaultsDetailPage({ params }: VaultsPageProps) {
  return (
    <div className="p-6">
      <h1 className="text-2xl font-bold">Vault {params.id}</h1>
      <p className="mt-2 text-gray-600 dark:text-gray-400">
        Vault details for: {params.id}
      </p>
    </div>
  );
}

export async function generateStaticParams() {
  return [];
}
