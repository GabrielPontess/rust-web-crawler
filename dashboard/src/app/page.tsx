import Link from "next/link";

export default function HomePage() {
  return (
    <div className="mx-auto max-w-3xl rounded-xl border bg-white p-10 shadow-sm">
      <h1 className="text-3xl font-semibold text-slate-900">Crawler Dashboard</h1>
      <p className="mt-4 text-slate-600">
        Bem-vindo ao painel em tempo real. Acompanhe as métricas do crawler, a fila
        de processamento e consulte o índice FTS diretamente.
      </p>
      <div className="mt-8 flex flex-wrap gap-4">
        <Link href="/dashboard" className="rounded bg-brand-600 px-4 py-2 text-white">
          Abrir Dashboard
        </Link>
        <Link
          href="/queue"
          className="rounded border border-brand-600 px-4 py-2 text-brand-600"
        >
          Ver fila
        </Link>
        <Link
          href="/search"
          className="rounded border border-slate-300 px-4 py-2 text-slate-700"
        >
          Buscar documentos
        </Link>
      </div>
    </div>
  );
}
