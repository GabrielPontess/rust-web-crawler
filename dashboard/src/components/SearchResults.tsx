import type { SearchResult } from "@/lib/types";

type Props = {
  results: SearchResult[];
};

export function SearchResults({ results }: Props) {
  if (!results.length) {
    return <p className="text-sm text-slate-500">Sem resultados.</p>;
  }

  return (
    <ul className="space-y-4">
      {results.map((result) => (
        <li key={result.url} className="rounded-xl border bg-white p-5 shadow-sm">
          <a
            href={`/pages/${encodeURIComponent(result.url)}`}
            className="text-lg font-semibold text-brand-600"
          >
            {result.title ?? result.url}
          </a>
          <p className="mt-1 text-xs text-slate-500">
            Score {result.score.toFixed(2)} · idioma {result.lang ?? "??"}
          </p>
          {result.snippet && (
            <p
              className="mt-2 text-sm text-slate-700"
              dangerouslySetInnerHTML={{ __html: result.snippet }}
            />
          )}
          <p className="mt-2 text-xs text-slate-400">{result.url}</p>
        </li>
      ))}
    </ul>
  );
}
