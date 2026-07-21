import { Injectable } from "@angular/core";
import { HttpClient } from "@angular/common/http";
import { Observable, of, forkJoin } from "rxjs";
import { catchError, map, switchMap, tap } from "rxjs/operators";
import { UserProfile } from "./profile.service";
import { resolveProfileName } from "../utils/profile-name";

export type EmploymentStatus =
  | "unemployed_need"
  | "unemployed_caln"
  | "badly_employed"
  | "employed_open";

export type ExperienceLevel =
  | "entry"
  | "first_role"
  | "junior"
  | "mid"
  | "senior"
  | "director"
  | "executive";

export type WorkType = "internship" | "full_time" | "part_time" | "contract";
export type WorkMode = "remote" | "onsite";
export type YesNoMaybe = "yes" | "no" | "prefer_not" | "";

export interface WorkAuthEntry {
  country: string;
  status: string;
}

export interface SalaryInfo {
  amount: number;
  currency: string;
}

export interface StudioPreferences {
  employmentStatus: EmploymentStatus | "";
  jobTitles: string[];
  experienceLevel: ExperienceLevel | "";
  salary: SalaryInfo;
  industries: string[];
  workTypes: WorkType[];
  startDate: string;
  willingToRelocate: boolean | null;
  workMode: WorkMode[];
  remoteLocations: string[];
  onsiteCities: string[];
  workAuth: WorkAuthEntry[];
  driversLicense: YesNoMaybe;
  gender: YesNoMaybe;
  disability: YesNoMaybe;
  veteran: YesNoMaybe;
  contactEmail: string;
  contactConsentNewsletter: boolean;
}

export interface StudioState {
  firstName: string;
  lastName: string;
  github: string;
  website: string;
  preferences: StudioPreferences;
  profile: UserProfile;
  screenshots: string[];
  completedAt: string | null;
}

export const DEFAULT_USERNAME = "default";

const STORAGE_KEY = "careertwin.studio";

@Injectable({ providedIn: "root" })
export class StudioService {
  private readonly api = "/api/v1";

  constructor(private http: HttpClient) {}

  emptyPreferences(): StudioPreferences {
    return {
      employmentStatus: "",
      jobTitles: [],
      experienceLevel: "",
      salary: { amount: 120000, currency: "PLN" },
      industries: [],
      workTypes: [],
      startDate: "",
      willingToRelocate: null,
      workMode: [],
      remoteLocations: [],
      onsiteCities: [],
      workAuth: [],
      driversLicense: "",
      gender: "",
      disability: "",
      veteran: "",
      contactEmail: "",
      contactConsentNewsletter: false,
    };
  }

  emptyState(): StudioState {
    return {
      firstName: "",
      lastName: "",
      github: "",
      website: "",
      preferences: this.emptyPreferences(),
      profile: {
        fullName: "",
        title: "",
        email: "",
        phone: "",
        location: "",
        linkedin: "",
        github: "",
        website: "",
        summary: "",
        skills: [],
        languages: [],
        experience: [],
        education: [],
        certificates: [],
        importedFromLinkedIn: false,
        updatedAt: "",
      },
      screenshots: [],
      completedAt: null,
    };
  }

  private normalize(raw: any): StudioState {
    const base = this.emptyState();
    const state = {
      ...base,
      ...raw,
      preferences: { ...base.preferences, ...(raw.preferences || {}) },
      profile: { ...base.profile, ...(raw.profile || {}) },
      salary: undefined as any,
    };
    state.profile.fullName = resolveProfileName(
      state.profile.fullName,
      state.firstName,
      state.lastName,
    );
    return state;
  }

  private mergeBackendData(local: StudioState, remote: any): StudioState {
    if (!remote?.exists && !remote?.preferences) return local;
    const remotePrefs = typeof remote.preferences === "string"
      ? JSON.parse(remote.preferences)
      : (remote.preferences || {});
    const remoteProfile = typeof remote.profile === "string"
      ? JSON.parse(remote.profile)
      : (remote.profile || {});

    const merged = { ...local };
    merged.github = remote.github || local.github;
    merged.website = remote.website || local.website;

    merged.preferences = { ...merged.preferences, ...remotePrefs };
    merged.profile = { ...merged.profile, ...remoteProfile };
    merged.firstName = remote.first_name || merged.firstName;
    merged.lastName = remote.last_name || merged.lastName;
    merged.profile.fullName = resolveProfileName(
      merged.profile.fullName,
      merged.firstName,
      merged.lastName,
    );

    if (remote.screenshots) merged.screenshots = remote.screenshots;
    if (remote.completed_at) merged.completedAt = remote.completed_at;

    return merged;
  }

  load(): StudioState | null {
    const localRaw = localStorage.getItem(STORAGE_KEY);
    const local = localRaw ? this.normalize(JSON.parse(localRaw)) : null;
    return local;
  }

  save(state: StudioState): void {
    state.completedAt = new Date().toISOString();
    localStorage.setItem(STORAGE_KEY, JSON.stringify(state));
  }

  clear(): void {
    localStorage.removeItem(STORAGE_KEY);
  }

  loadFromBackend(): Observable<StudioState> {
    return this.http.get<any>(`${this.api}/studio/state`, {
      params: { username: DEFAULT_USERNAME },
    }).pipe(
      map((remote) => {
        if (!remote?.exists) return this.emptyState();
        const remotePrefs = typeof remote.preferences === "string"
          ? JSON.parse(remote.preferences)
          : (remote.preferences || {});
        const remoteProfile = typeof remote.profile === "string"
          ? JSON.parse(remote.profile)
          : (remote.profile || {});

        const state = this.emptyState();
        state.firstName = remote.first_name || "";
        state.lastName = remote.last_name || "";
        state.github = remote.github || "";
        state.website = remote.website || "";
        state.preferences = { ...state.preferences, ...remotePrefs };
        state.profile = { ...state.profile, ...remoteProfile };
        state.profile.fullName = resolveProfileName(
          state.profile.fullName,
          state.firstName,
          state.lastName,
        );
        state.completedAt = remote.completed_at || null;

        localStorage.setItem(STORAGE_KEY, JSON.stringify(state));
        return state;
      }),
      catchError(() => {
        const local = this.load();
        return of(local || this.emptyState());
      }),
    );
  }

  submit(state: StudioState): Observable<any> {
    const payload = {
      username: DEFAULT_USERNAME,
      first_name: state.firstName || null,
      last_name: state.lastName || null,
      github: state.github || null,
      website: state.website || null,
      preferences: state.preferences,
      profile: state.profile,
      screenshots: state.screenshots || [],
      completed_at: state.completedAt,
    };
    return this.http.post(`${this.api}/studio/submit`, payload).pipe(
      tap((res: any) => {
        if (res?.success) {
          state.completedAt = new Date().toISOString();
          localStorage.setItem(STORAGE_KEY, JSON.stringify(state));
        }
      }),
      catchError(() => of({ success: false })),
    );
  }

  getState(username: string = DEFAULT_USERNAME): Observable<any> {
    return this.http
      .get(`${this.api}/studio/state`, { params: { username } })
      .pipe(catchError(() => of({ exists: false })));
  }

}
