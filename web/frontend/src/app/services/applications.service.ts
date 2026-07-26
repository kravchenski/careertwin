import { Injectable } from "@angular/core";
import { HttpClient } from "@angular/common/http";
import { Observable, of } from "rxjs";
import { catchError } from "rxjs/operators";

export interface ApplicationTimelineStep {
  label: string;
  date: string;
  completed: boolean;
  active: boolean;
}

export interface ApplicationNote {
  content: string;
  date: string;
}

export interface Application {
  id: string;
  company: string;
  position: string;
  domain: string;
  job_url: string;
  status: string;
  status_label: string;
  timeline: ApplicationTimelineStep[];
  notes: ApplicationNote[];
  created_at: string;
  updated_at: string;
}

export interface ApplicationStage {
  name: string;
  count: number;
  percentage: number;
}

export interface ApplicationsListResponse {
  success: boolean;
  applications: Application[];
  pipeline: ApplicationStage[];
}

@Injectable({ providedIn: "root" })
export class ApplicationsService {
  private readonly api = "/api/v1";

  constructor(private http: HttpClient) {}

  list(username: string = "default"): Observable<ApplicationsListResponse> {
    return this.http
      .get<ApplicationsListResponse>(`${this.api}/applications`, { params: { username } })
      .pipe(catchError(() => of({ success: false, applications: [], pipeline: [] })));
  }

  create(payload: {
    username?: string;
    company: string;
    position: string;
    domain?: string;
    job_url?: string;
  }): Observable<{ success: boolean; application?: Application }> {
    return this.http
      .post<{ success: boolean; application?: Application }>(`${this.api}/applications`, payload)
      .pipe(catchError(() => of({ success: false })));
  }

  update(
    id: string,
    payload: { status?: string; note?: string },
  ): Observable<{ success: boolean; application?: Application }> {
    return this.http
      .patch<{ success: boolean; application?: Application }>(`${this.api}/applications/${id}`, payload)
      .pipe(catchError(() => of({ success: false })));
  }

  remove(id: string): Observable<{ success: boolean }> {
    return this.http
      .delete<{ success: boolean }>(`${this.api}/applications/${id}`)
      .pipe(catchError(() => of({ success: false })));
  }
}
