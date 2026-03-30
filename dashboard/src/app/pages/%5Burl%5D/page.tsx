import { getPageDetail } from "@/lib/api";
import type { PageDetail } from "@/lib/types";

type Props = {
  params: {
    url: string;
  };
};

export default async function PageDetailPage({ params }: Props) {
  const decoded = decodeURIComponent(params.url);
  const detail = await getPageDetail(decoded);

  return (
    <div className="space-y-6">
      <header>
        <p className="text-xs text-slate-400">Documento</p>
        <h1 className="text-2xl font-semibold text-slate-900">{detail.title ?? decoded}</h1>
        <a href={detail.url} className="text-sm text-brand-600" target="_blank">
          {detail.url}
        </a>
        <p className="text-xs text-slate-400">
          {detail.lang ?? "??"} · {detail.crawled_at ?? "sem timestamp"}
        </p>
      </header>

      {detail.description && (
        <div className="rounded-lg border bg-white p-4 shadow-sm">
          <h2 className="text-sm font-semibold text-slate-700">Descrição</h2>
          <p className="text-sm text-slate-600">{detail.description}</p>
        </div>
      )}

      {detail.headings.length > 0 && (
        <div className="rounded-lg border bg-white p-4 shadow-sm">
          <h2 className="text-sm font-semibold text-slate-700">Headings</h2>
          <ul className="mt-2 list-disc space-y-1 pl-4 text-sm text-slate-600">
            {detail.headings.map((heading) => (
              <li key={heading}>{heading}</li>
            ))}
          </ul>
        </div>
      )}

      {detail.summary && (
        <div className="rounded-lg border bg-white p-4 shadow-sm">
          <h2 className="text-sm font-semibold text-slate-700">Resumo</h2>
          <p className="text-sm text-slate-600">{detail.summary}</p>
        </div>
      )}

      {detail.content && (
        <div className="rounded-lg border bg-white p-4 shadow-sm">
          <h2 className="text-sm font-semibold text-slate-700">Conteúdo bruto</h2>
          <p className="whitespace-pre-wrap text-sm text-slate-600">{detail.content}</p>
        </div>
      )}
    </div>
  );
}

export async function generateStaticParams() {
  return [];
}
