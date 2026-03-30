"use client";

import { useState } from "react";

import { searchDocuments } from "@/lib/api";
import type { SearchResult } from "@/lib/types";
import { SearchResults } from "./SearchResults";

export function SearchForm() {
  const [query, setQuery] = useState("rust async");
  const [loading, setLoading] = useState(false);
  const [results, setResults] = useState<SearchResult[]>([]);
  const [error, setError] = useState<string | null>(null);

  async function handleSubmit(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setLoading(true);
    setError(null);
    try {
      const data = await searchDocuments(query, 10);
      setResults(data);
    } catch (err) {
      setError("Falha ao consultar o índice");
    } finally {
      setLoading(false);
    }
  }

  return (
    <div className="space-y-6">
      <form onSubmit={handleSubmit} className="flex gap-3">
        <input
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          className="flex-1 rounded-lg border px-4 py-2"
          placeholder="tokens, frases ou domínios"
        />
        <button
          type="submit"
          className="rounded-lg bg-brand-600 px-6 py-2 font-medium text-white"
          disabled={loading}
        >
          {loading ? "Buscando..." : "Buscar"}
        </button>
      </form>
      {error && <p className="text-sm text-rose-600">{error}</p>}
      <SearchResults results={results} />
    </div>
  );
}
