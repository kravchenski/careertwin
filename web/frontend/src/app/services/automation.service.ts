import { Injectable, inject } from "@angular/core";
import { HttpClient } from "@angular/common/http";

export interface AgentSettings {
  context: string; queries: string[]; sources: string[];
  companies: Record<string, string>[]; preferences: string;
  provider: string; model: string; jev_model: string;
  decision_backend: 'switchyard' | 'typesafe'; decision_model: string;
  threshold: number; confidence: number; auto_prepare: boolean;
  auto_submit: boolean; scheduled: boolean; interval_hours: number;
  limit: number; daily_limit: number;
}
export interface Vacancy {
  url: string; title: string; company: string; source: string;
  location: string; description: string; salary: string;
}
export interface AgentJob {
  id: string; state: string; error: string; vacancy: Vacancy;
  evaluation: { score: number; confidence: number; category: string; gaps: string[]; unknown: string[]; confidence_method: string; model: string } | null;
  package: { revision: string; cv_path: string; cv_url: string; checks: { pages: number } } | null;
  updated_at: string;
}
export interface AgentEvent { id: number; stage: string; created_at: string; payload: Record<string, unknown>; }
export interface AgentStatus { running: boolean; active_jobs: string[]; last_run: string | null; decision_configured: boolean; decision_backend: string; submitted_today: number; excel_path: string; excel_error: string; }
export interface AgentDetail { job: AgentJob; events: AgentEvent[]; files: { kind: string; path: string; sha256: string }[]; }

@Injectable({ providedIn: "root" })
export class AutomationService {
  private http = inject(HttpClient);
  private base = "/api/v1/automation";
  settings() { return this.http.get<AgentSettings>(`${this.base}/settings`); }
  save(settings: AgentSettings) { return this.http.put<AgentSettings>(`${this.base}/settings`, settings); }
  sources() { return this.http.get<{ id: string; domain: string; method: string }[]>(`${this.base}/sources`); }
  decisionModels() { return this.http.get<{id:string}[]>(`${this.base}/decision-models`); }
  jobs() { return this.http.get<AgentJob[]>(`${this.base}/jobs`); }
  status() { return this.http.get<AgentStatus>(`${this.base}/status`); }
  detail(id: string) { return this.http.get<AgentDetail>(`${this.base}/jobs/${id}`); }
  import(vacancy: Partial<Vacancy>) { return this.http.post<AgentJob>(`${this.base}/jobs`, vacancy); }
  run() { return this.http.post(`${this.base}/run`, {}); }
  stop() { return this.http.post(`${this.base}/stop`, {}); }
  action(id: string, operation: string) { return this.http.post(`${this.base}/jobs/${id}/actions`, { operation }); }
  answer(id: string, question: string, answer: string) { return this.http.post(`${this.base}/jobs/${id}/answers`, { question, answer }); }
  outcome(id: string, status: string, note = "") { return this.http.post(`${this.base}/jobs/${id}/outcome`, { status, note }); }
  exportUrl = `${this.base}/export.xlsx`;
}
