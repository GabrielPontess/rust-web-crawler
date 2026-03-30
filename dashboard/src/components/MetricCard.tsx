type Props = {
  label: string;
  value: number | string;
  accent?: "primary" | "danger";
  helper?: string;
};

export function MetricCard({ label, value, accent = "primary", helper }: Props) {
  const accentClass =
    accent === "danger" ? "text-rose-600 bg-rose-50" : "text-brand-600 bg-brand-50";
  return (
    <div className="rounded-xl border bg-white p-5 shadow-sm">
      <p className="text-sm text-slate-500">{label}</p>
      <p className={`mt-2 text-3xl font-semibold ${accentClass}`}>{value}</p>
      {helper && <p className="mt-1 text-xs text-slate-400">{helper}</p>}
    </div>
  );
}
