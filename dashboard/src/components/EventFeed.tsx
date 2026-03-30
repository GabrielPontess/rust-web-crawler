"use client";

import { useEffect, useState } from "react";

import { API_BASE_URL, API_TOKEN } from "@/lib/config";
import type { EventLog } from "@/lib/types";

type Props = {
  initialEvents: EventLog[];
};

type LiveEvent = {
  type: string;
  url: string;
  host?: string;
  message?: string;
  duration_ms?: number;
  attempts?: number;
  timestamp: string;
};

export function EventFeed({ initialEvents }: Props) {
  const [events, setEvents] = useState<EventLog[]>(initialEvents);

  useEffect(() => {
    const base = `${API_BASE_URL}/stream/events`;
    const url = API_TOKEN ? `${base}?token=${API_TOKEN}` : base;
    const source = new EventSource(url);
    attach(source);
    return () => source.close();

    function attach(stream: EventSource) {
      stream.addEventListener("message", (evt) => {
        try {
          const payload = JSON.parse(evt.data) as LiveEvent;
          setEvents((prev) => {
            const next: EventLog = {
              id: Date.now(),
              event_type: payload.type,
              url: payload.url,
              host: payload.host,
              message: payload.message,
              duration_ms: payload.duration_ms ? Math.round(payload.duration_ms) : undefined,
              attempts: payload.attempts,
              created_at: payload.timestamp,
            };
            return [next, ...prev].slice(0, 50);
          });
        } catch (error) {
          console.error("Unable to parse event", error);
        }
      });
    }
  }, []);

  return (
    <div className="space-y-3">
      {events.map((event) => (
        <div key={`${event.id}-${event.created_at}`} className="rounded-lg border bg-white p-4">
          <div className="flex items-center justify-between text-xs text-slate-500">
            <span className="font-semibold uppercase">{event.event_type}</span>
            <span>{new Date(event.created_at).toLocaleTimeString()}</span>
          </div>
          <p className="mt-2 text-sm">
            <a href={`/pages/${encodeURIComponent(event.url)}`} className="text-brand-600">
              {event.url}
            </a>
          </p>
          {event.message && <p className="text-xs text-rose-600">{event.message}</p>}
          {event.host && <p className="text-xs text-slate-500">host: {event.host}</p>}
        </div>
      ))}
    </div>
  );
}
