import { Injectable } from "@angular/core";
import { HttpClient } from "@angular/common/http";
import { Observable } from "rxjs";

export interface ResumeTailorForm {
  mode: "upload" | "manual";
  resumeFile?: File | null;
  fullName: string;
  email: string;
  phone: string;
  location: string;
  github: string;
  linkedin: string;
  website: string;
  summary: string;
  jobUrl: string;
  jobTitle: string;
  companyName: string;
  jobDescription: string;
  template: string;
  provider: string;
  model: string;
  language: string;
}

export interface AutoTailorResponse {
  success: boolean;
  query: string;
  jobs_found: number;
  jobs_processed: number;
  sources: string[];
  packages: TailorResponse[];
  agent_diagnostics: Record<string, {
    status: string;
    detail: string;
    [key: string]: unknown;
  }>;
}

export interface DiscoveredJob {
  source: string;
  url: string;
  title: string;
  company: string;
  description: string;
  match_score: number;
  matching_skills: string[];
  missing_skills: string[];
}

export interface DiscoverJobsResponse {
  success: boolean;
  query: string;
  total: number;
  jobs: DiscoveredJob[];
  agent_diagnostics: Record<string, {
    status: string;
    detail: string;
    [key: string]: unknown;
  }>;
}

export interface ApplyJobResponse {
  success: boolean;
  url?: string;
  filled_fields?: string[];
  status?: string;
  message?: string;
  error?: string;
  agent_trace?: Array<{
    step: number;
    status: string;
    action: string;
    target?: string;
    value_preview?: string;
    reason?: string;
    url?: string;
    title?: string;
    command_ok?: boolean;
    command_output?: string;
  }>;
  page_info?: {
    forms_found: number;
    inputs_found: number;
    apply_buttons: number;
  };
  browser_session?: {
    status?: string;
    detail?: string;
    provider?: string;
    session?: string;
  };
}

export interface ProviderInfo {
  id: string;
  default_model: string;
}

export interface ProviderStatus extends ProviderInfo {
  status: string;
  reason?: string;
  key_env?: string;
  available_models?: string[];
}

export interface TailorResponse {
  success: boolean;
  package_id: string;
  pdf_url: string;
  html_url: string;
  cover_letter_pdf_url: string;
  ats_score: number;
  job_match_score: number;
  readiness_score: number;
  language: string;
  language_label: string;
  source?: string;
  initial_match_score?: number;
  company_research: {
    language: string;
    reasoning: string;
    company_summary: string;
  };
  job: {
    title: string;
    company: string;
    description: string;
    skills: string[];
    url: string;
  };
  resume: {
    full_name: string;
    summary: string;
    text: string;
    skills: string[];
    projects: Array<{
      name: string;
      description: string;
      url?: string;
    }>;
    experience: Array<{
      company: string;
      position: string;
      description: string;
    }>;
  };
  cover_letter: string;
  agent_diagnostics: Record<string, {
    status: string;
    detail: string;
    [key: string]: unknown;
  }>;
  reports: {
    ats_passed: boolean;
    ats_issues: string[];
    missing_skills: string[];
    skill_gaps: string[];
    match_recommendation: string;
  };
}

@Injectable({ providedIn: "root" })
export class ResumeService {
  private readonly api = "/api/v1";

  constructor(private http: HttpClient) {}

  tailorResume(payload: ResumeTailorForm): Observable<TailorResponse> {
    const formData = new FormData();
    formData.append("mode", payload.mode);
    if (payload.resumeFile) {
      formData.append("resume_file", payload.resumeFile);
    }
    formData.append("fullName", payload.fullName);
    formData.append("email", payload.email);
    formData.append("phone", payload.phone);
    formData.append("location", payload.location);
    formData.append("github", payload.github);
    formData.append("linkedin", payload.linkedin);
    formData.append("website", payload.website);
    formData.append("summary", payload.summary);
    formData.append("jobUrl", payload.jobUrl);
    formData.append("jobTitle", payload.jobTitle);
    formData.append("companyName", payload.companyName);
    formData.append("jobDescription", payload.jobDescription);
    formData.append("template", payload.template);
    formData.append("provider", payload.provider);
    formData.append("model", payload.model);
    formData.append("language", payload.language);
    return this.http.post<TailorResponse>(`${this.api}/resume/tailor`, formData);
  }

  autoTailorResume(payload: ResumeTailorForm, limit: number, sources: string[]): Observable<AutoTailorResponse> {
    const formData = new FormData();
    formData.append("mode", payload.mode);
    if (payload.resumeFile) {
      formData.append("resume_file", payload.resumeFile);
    }
    formData.append("fullName", payload.fullName);
    formData.append("email", payload.email);
    formData.append("phone", payload.phone);
    formData.append("location", payload.location);
    formData.append("github", payload.github);
    formData.append("linkedin", payload.linkedin);
    formData.append("website", payload.website);
    formData.append("summary", payload.summary);
    formData.append("template", payload.template);
    formData.append("provider", payload.provider);
    formData.append("model", payload.model);
    formData.append("language", payload.language);
    formData.append("limit", String(limit));
    formData.append("sources", sources.join(","));
    return this.http.post<AutoTailorResponse>(`${this.api}/resume/auto-tailor`, formData);
  }

  discoverJobs(payload: ResumeTailorForm, limit: number, sources: string[]): Observable<DiscoverJobsResponse> {
    const formData = new FormData();
    formData.append("mode", payload.mode);
    if (payload.resumeFile) {
      formData.append("resume_file", payload.resumeFile);
    }
    formData.append("fullName", payload.fullName);
    formData.append("email", payload.email);
    formData.append("phone", payload.phone);
    formData.append("location", payload.location);
    formData.append("github", payload.github);
    formData.append("linkedin", payload.linkedin);
    formData.append("website", payload.website);
    formData.append("summary", payload.summary);
    formData.append("limit", String(limit));
    formData.append("sources", sources.join(","));
    return this.http.post<DiscoverJobsResponse>(`${this.api}/jobs/discover`, formData);
  }

  applyToJob(payload: {
    job_url: string;
    username?: string;
    full_name: string;
    email: string;
    phone: string;
    cover_letter: string;
    provider?: string;
    model?: string;
    auto_submit?: boolean;
  }): Observable<ApplyJobResponse> {
    return this.http.post<ApplyJobResponse>(`${this.api}/jobs/apply`, {
      username: payload.username || "local-user",
      job_url: payload.job_url,
      full_name: payload.full_name,
      email: payload.email,
      phone: payload.phone,
      cover_letter: payload.cover_letter,
      provider: payload.provider || "lmstudio",
      model: payload.model || "",
      auto_submit: payload.auto_submit || false,
    });
  }

  getTemplates(): Observable<{ templates: Array<{ id: string; name: string }> }> {
    return this.http.get<{ templates: Array<{ id: string; name: string }> }>(`${this.api}/templates`);
  }

  getLanguages(): Observable<{ languages: Array<{ code: string; name: string }> }> {
    return this.http.get<{ languages: Array<{ code: string; name: string }> }>(`${this.api}/languages`);
  }

  getProviders(): Observable<{ providers: ProviderInfo[] }> {
    return this.http.get<{ providers: ProviderInfo[] }>(`${this.api}/providers`);
  }

  getProviderStatuses(): Observable<{ providers: ProviderStatus[] }> {
    return this.http.get<{ providers: ProviderStatus[] }>(`${this.api}/providers/status`);
  }
}
