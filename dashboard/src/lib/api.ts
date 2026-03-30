import { API_BASE_URL, API_TOKEN } from "./config";
import type {
  EventLog,
  MetricsResponse,
  PageDetail,
  QueueItem,
  SearchResult,
} from "./types";

async function apiFetch<T>(path: string, fallback?: T): Promise<T> {
  const url = `${API_BASE_URL}${path}`;
  try {
    const res = await fetch(url, {
      headers: API_TOKEN ? { "x-api-key": API_TOKEN } : undefined,
      cache: "no-store",
    });
    if (!res.ok) {
      throw new Error(`API ${path} failed: ${res.status}`);
    }
    return res.json();
  } catch (error) {
    console.error("API request failed", path, error);
    if (fallback !== undefined) {
      return fallback;
    }
    throw error;
  }
}

export const getMetrics = () =>
  apiFetch<MetricsResponse>("/api/metrics", {
    total_pages: 0,
    queue_pending: 0,
    queue_processing: 0,
    queue_failed: 0,
  });

export const getQueue = (params: { status?: string; host?: string; limit?: number }) => {
  const query = new URLSearchParams();
  if (params.status) query.set("status", params.status);
  if (params.host) query.set("host", params.host);
  if (params.limit) query.set("limit", params.limit.toString());
  return apiFetch<QueueItem[]>(`/api/queue?${query.toString()}`, []);
};

export const getEvents = (limit = 50) =>
  apiFetch<EventLog[]>(`/api/events?limit=${limit}`, []);

export const searchDocuments = (query: string, limit = 10) => {
  const search = new URLSearchParams({ query, limit: limit.toString() });
  return apiFetch<SearchResult[]>(`/api/search?${search.toString()}`, []);
};

export const getPageDetail = (url: string) =>
  apiFetch<PageDetail>(`/api/page?${new URLSearchParams({ url })}`);
