import type { Metadata } from "next";
import "./globals.css";

export const metadata: Metadata = {
  title: "Crawler Dashboard",
  description: "Monitoring UI for the personal search crawler",
};

type Props = {
  children: React.ReactNode;
};

export default function RootLayout({ children }: Props) {
  return (
    <html lang="en">
      <body className="min-h-screen bg-gray-50 text-slate-900">
        <div className="flex min-h-screen">
          <aside className="hidden w-64 flex-col border-r bg-white p-6 lg:flex">
            <div className="mb-8 text-2xl font-semibold text-brand-600">
              Crawler Dashboard
            </div>
            <nav className="flex flex-1 flex-col gap-2 text-sm">
              <a href="/dashboard" className="rounded px-3 py-2 hover:bg-brand-50">
                Dashboard
              </a>
              <a href="/queue" className="rounded px-3 py-2 hover:bg-brand-50">
                Queue
              </a>
              <a href="/search" className="rounded px-3 py-2 hover:bg-brand-50">
                Search
              </a>
            </nav>
            <div className="mt-auto text-xs text-slate-500">
              Powered by Rust crawler
            </div>
          </aside>
          <main className="flex-1 p-6 lg:p-10">{children}</main>
        </div>
      </body>
    </html>
  );
}
