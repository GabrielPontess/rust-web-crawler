import { SearchForm } from "@/components/SearchForm";

export default function SearchPage() {
  return (
    <div className="space-y-6">
      <header>
        <h1 className="text-3xl font-semibold text-slate-900">Busca</h1>
        <p className="text-sm text-slate-500">
          Consulta o índice FTS mantido pelo crawler. Os resultados exibem título,
          snippet e idioma detectado.
        </p>
      </header>
      <SearchForm />
    </div>
  );
}
