import { apiGet } from "@/lib/api-client";

export interface BackfillStatus {
  initialized: boolean;
  complete: boolean;
  fraction: number;
  percent_complete: number;
  events_ingested: number;
  total_estimate?: number | null;
  total_final?: boolean;
  counting?: boolean;
  window_start?: string | null;
  window_end?: string | null;
  cursor?: string | null;
  started_at?: string | null;
  completed_at?: string | null;
  progress_updated_at?: string | null;
}

export interface ConnectorStatus {
  source_key: string;
  source_name: string;
  enabled: boolean;
  last_live_run_at?: string | null;
  backfill: BackfillStatus;
}

export interface ConnectorStatusResponse {
  connectors: ConnectorStatus[];
}

export async function listConnectors(accessToken: string): Promise<ConnectorStatusResponse> {
  return apiGet<ConnectorStatusResponse>("/api/v1/connectors", accessToken);
}
