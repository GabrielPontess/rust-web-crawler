import { EventFeed } from "@/components/EventFeed";
import { MetricCard } from "@/components/MetricCard";
import { getEvents, getMetrics } from "@/lib/api";

export default async function DashboardPage() {
  const [metrics, events] = await Promise.all([getMetrics(), getEvents(10)]);

  return (
    <div className="space-y-8">
      <header>
        <h1 className="text-3xl font-semibold text-slate-900">Dashboard</h1>
        <p className="text-sm text-slate-500">
          A visão geral do pipeline em tempo real. Dados atualizados diretamente do
          crawler.
        </p>
      </header>

      <section className="grid gap-4 md:grid-cols-2 lg:grid-cols-4">
        <MetricCard label="Páginas indexadas" value={metrics.total_pages} />
        <MetricCard label="Fila pendente" value={metrics.queue_pending} />
        <MetricCard label="Em processamento" value={metrics.queue_processing} />
        <MetricCard label="Falhas" value={metrics.queue_failed} accent="danger" />
      </section>

      <section className="grid gap-6 lg:grid-cols-2">
        <div>
          <h2 className="text-lg font-semibold text-slate-800">Eventos recentes</h2>
          <p className="text-sm text-slate-500">
            Feed em tempo real (SSE) com as últimas atividades do crawler.
          </p>
          <div className="mt-4">
            <EventFeed initialEvents={events} />
          </div>
        </div>
      </section>
    </div>
  );
}
