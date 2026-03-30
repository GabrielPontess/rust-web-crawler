import type { QueueItem } from "@/lib/types";

type Props = {
  items: QueueItem[];
};

export function QueueTable({ items }: Props) {
  if (!items.length) {
    return <p className="text-sm text-slate-500">Nenhum item encontrado.</p>;
  }

  return (
    <div className="overflow-hidden rounded-xl border bg-white shadow-sm">
      <table className="w-full text-left text-sm">
        <thead className="bg-slate-100 text-xs uppercase tracking-wide text-slate-500">
          <tr>
            <th className="px-4 py-3">URL</th>
            <th className="px-4 py-3">Status</th>
            <th className="px-4 py-3">Tentativas</th>
            <th className="px-4 py-3">Host</th>
            <th className="px-4 py-3">Erro</th>
          </tr>
        </thead>
        <tbody>
          {items.map((item) => (
            <tr key={item.url} className="border-t">
              <td className="px-4 py-3 text-brand-600">
                <a href={`/pages/${encodeURIComponent(item.url)}`}>{item.url}</a>
              </td>
              <td className="px-4 py-3">
                <span className="rounded-full bg-slate-100 px-2 py-1 text-xs font-medium">
                  {item.status}
                </span>
              </td>
              <td className="px-4 py-3">{item.attempts}</td>
              <td className="px-4 py-3">{item.host ?? "—"}</td>
              <td className="px-4 py-3 text-rose-600">
                {item.last_error ? item.last_error.slice(0, 60) : "—"}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
