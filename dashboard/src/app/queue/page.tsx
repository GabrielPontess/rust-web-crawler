import { QueueTable } from "@/components/QueueTable";
import { getQueue } from "@/lib/api";

export default async function QueuePage() {
  const queue = await getQueue({ status: "pending", limit: 50 });
  return (
    <div className="space-y-6">
      <header>
        <h1 className="text-3xl font-semibold text-slate-900">Fila</h1>
        <p className="text-sm text-slate-500">
          URLs aguardando processamento. Ajuste filtros alterando a chamada em
          `src/app/queue/page.tsx` ou adicione um formulário futuramente.
        </p>
      </header>
      <QueueTable items={queue} />
    </div>
  );
}
