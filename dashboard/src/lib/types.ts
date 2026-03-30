export type MetricsResponse = {
  total_pages: number;
  queue_pending: number;
  queue_processing: number;
  queue_failed: number;
};

export type QueueItem = {
  url: string;
  status: string;
  priority: number;
  attempts: number;
  last_error?: string;
  host?: string;
  next_run_at?: string;
  created_at?: string;
};

export type EventLog = {
  id: number;
  event_type: string;
  url: string;
  host?: string;
  message?: string;
  duration_ms?: number;
  attempts?: number;
  created_at: string;
};

export type SearchResult = {
  url: string;
  title?: string;
  snippet?: string;
  lang?: string;
  score: number;
};

export type PageDetail = {
  url: string;
  title?: string;
  description?: string;
  headings: string[];
  content?: string;
  summary?: string;
  lang?: string;
  crawled_at?: string;
};
